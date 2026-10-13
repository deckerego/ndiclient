//! NDI client: discovers the NDI source with a given device and stream name
//! and shows its video in a desktop window. Without arguments it lists the
//! NDI sources it discovers.

mod display;
mod frame_rate;
mod ndi;
mod picture;
mod stall;

use std::process::ExitCode;
use std::time::Duration;

use display::VideoWindow;
use frame_rate::FrameRateMeter;
use ndi::{Capture, NdiRuntime, Receiver, SourceFinder, SourceQuery};
use stall::StallDetector;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for sources on a typical LAN to answer mDNS discovery.
const LISTING_DURATION: Duration = Duration::from_secs(5);
/// Short enough to keep the window responsive while no frames arrive.
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(100);
const FRAME_RATE_REPORT_INTERVAL: Duration = Duration::from_secs(5);
const STALL_THRESHOLD: Duration = Duration::from_secs(3);
const USAGE: &str = "usage: ndiclient <device-name> <stream-name>
       ndiclient                              (list discovered sources)";

enum Command {
    ListSources,
    ShowVideo(SourceQuery),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ndiclient: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let command = parse_arguments(&arguments)?;
    let runtime = NdiRuntime::initialize()?;
    let finder = SourceFinder::new(&runtime)?;
    match command {
        Command::ListSources => {
            list_sources(&finder);
            Ok(())
        }
        Command::ShowVideo(query) => show_video(&runtime, &finder, &query),
    }
}

fn parse_arguments(arguments: &[String]) -> Result<Command, String> {
    match arguments {
        [] => Ok(Command::ListSources),
        [device_name, stream_name] => Ok(Command::ShowVideo(SourceQuery::new(
            device_name.clone(),
            stream_name.clone(),
        ))),
        _ => Err(USAGE.to_string()),
    }
}

fn list_sources(finder: &SourceFinder) {
    println!("Discovering NDI sources for {LISTING_DURATION:?}...");
    let names = finder.source_names_after(LISTING_DURATION);
    if names.is_empty() {
        println!("No NDI sources were discovered");
    }
    for name in names {
        println!("{name}");
    }
}

fn show_video(
    runtime: &NdiRuntime,
    finder: &SourceFinder,
    query: &SourceQuery,
) -> Result<(), String> {
    println!("Searching for {query}...");
    let source = finder.find_source(query, DISCOVERY_TIMEOUT)?;
    println!("Connecting to \"{}\"", source.name());

    let receiver = Receiver::connect(runtime, &source)?;
    let mut window = VideoWindow::open(&source.name())?;
    stream_video(&receiver, &mut window)
}

fn stream_video(receiver: &Receiver, window: &mut VideoWindow) -> Result<(), String> {
    let mut frame_rate = FrameRateMeter::new(FRAME_RATE_REPORT_INTERVAL);
    let mut stall = StallDetector::new(STALL_THRESHOLD);
    while !window.is_close_requested() {
        match receiver.capture_video(CAPTURE_TIMEOUT) {
            Capture::Video(frame) => {
                show_frame(&frame, window)?;
                stall.record_frame();
                report_frame_rate(&mut frame_rate);
            }
            Capture::ConnectionLost => {
                return Err("connection to the NDI source was lost".to_string());
            }
            Capture::Nothing => report_stall(&mut stall),
        }
    }
    Ok(())
}

fn show_frame(frame: &ndi::VideoFrame, window: &mut VideoWindow) -> Result<(), String> {
    match frame.picture() {
        Some(picture) => window.show(&picture),
        None => Err("the NDI source sent an unsupported pixel format".to_string()),
    }
}

fn report_frame_rate(frame_rate: &mut FrameRateMeter) {
    if let Some(frames_per_second) = frame_rate.record_frame() {
        println!("Displaying {frames_per_second:.1} frames per second");
    }
}

fn report_stall(stall: &mut StallDetector) {
    if stall.take_new_stall() {
        println!("Waiting for video...");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn no_arguments_lists_sources() {
        let command = parse_arguments(&arguments(&[]));
        assert!(matches!(command, Ok(Command::ListSources)));
    }

    #[test]
    fn device_and_stream_names_show_video() {
        let command = parse_arguments(&arguments(&["STUDIO", "OBS PGM"]));
        assert!(matches!(command, Ok(Command::ShowVideo(_))));
    }

    #[test]
    fn single_argument_is_rejected() {
        assert!(parse_arguments(&arguments(&["STUDIO"])).is_err());
    }

    #[test]
    fn extra_argument_is_rejected() {
        assert!(parse_arguments(&arguments(&["STUDIO", "OBS PGM", "extra"])).is_err());
    }
}
