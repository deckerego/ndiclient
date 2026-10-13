//! NDI client: discovers the NDI source with a given device and stream name
//! and shows its video in a desktop window.

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
/// Short enough to keep the window responsive while no frames arrive.
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(100);
const FRAME_RATE_REPORT_INTERVAL: Duration = Duration::from_secs(5);
const STALL_THRESHOLD: Duration = Duration::from_secs(3);
const USAGE: &str = "usage: ndiclient <device-name> <stream-name>";

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
    let query = parse_arguments()?;
    let runtime = NdiRuntime::initialize()?;

    println!("Searching for {query}...");
    let source = SourceFinder::new(&runtime, query)?.find_source(DISCOVERY_TIMEOUT)?;
    println!("Connecting to \"{}\"", source.name());

    let receiver = Receiver::connect(&runtime, &source)?;
    let mut window = VideoWindow::open(&source.name())?;
    stream_video(&receiver, &mut window)
}

fn parse_arguments() -> Result<SourceQuery, String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [device_name, stream_name] => {
            Ok(SourceQuery::new(device_name.clone(), stream_name.clone()))
        }
        _ => Err(USAGE.to_string()),
    }
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
