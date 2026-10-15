//! NDI client: discovers the NDI source with a given device name (and
//! optionally stream name), shows its video in a desktop window or
//! fullscreen and plays its audio. Without arguments it lists the NDI
//! sources it discovers. `--ipaddr` restricts both to one IP address.

mod audio_pump;
mod bridge;
mod display;
mod dropout;
mod frame_rate;
mod i420;
mod jitter_buffer;
mod ndi;
mod picture;
mod sound;
mod speaker;
mod stall;

use std::collections::HashSet;
use std::net::IpAddr;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use display::{VideoWindow, WindowMode, WindowStatus};
use frame_rate::FrameRateMeter;
use jitter_buffer::{AudioLatency, JitterBuffer};
use ndi::{Capture, DiscoveryScope, NdiRuntime, Receiver, SourceFinder, SourceQuery};
use speaker::Speaker;
use stall::StallDetector;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for sources on a typical LAN to answer mDNS discovery.
const LISTING_DURATION: Duration = Duration::from_secs(5);
/// Short enough to keep the window responsive while no frames arrive.
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(100);
const FRAME_RATE_REPORT_INTERVAL: Duration = Duration::from_secs(5);
const STALL_THRESHOLD: Duration = Duration::from_secs(3);
/// Audio plays this far behind its arrival. Unbroken audio matters more
/// than closeness to the video (shown as it arrives), so it starts beyond
/// the arrival gaps seen on a loaded Wi-Fi and underruns raise it further.
const AUDIO_LATENCY: AudioLatency = AudioLatency {
    start: Duration::from_millis(100),
    limit: Duration::from_millis(400),
    headroom: Duration::from_millis(150),
};
/// Short enough not to be heard as a fade, long enough not to click.
const AUDIO_FADE: Duration = Duration::from_millis(5);
const FULLSCREEN_OPTION: &str = "--fullscreen";
const NO_AUDIO_OPTION: &str = "--noaudio";
const IP_ADDRESS_OPTION: &str = "--ipaddr";
const OPTION_PREFIX: &str = "--";
const USAGE: &str =
    "usage: ndiclient [--fullscreen] [--noaudio] [--ipaddr <ip>] <device-name> [stream-name]
       ndiclient [--ipaddr <ip>]          (list discovered sources)";

struct Invocation {
    scope: DiscoveryScope,
    command: Command,
}

enum Command {
    ListSources,
    ShowVideo {
        query: SourceQuery,
        options: ViewOptions,
    },
}

/// The command line sorted into its parts, before they are interpreted.
struct SortedArguments<'a> {
    names: Vec<&'a String>,
    view_options: Vec<&'a String>,
    scope: DiscoveryScope,
}

/// Whether the source's audio is played. Without it, no audio device is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AudioMode {
    Play,
    Mute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewOptions {
    window_mode: WindowMode,
    audio_mode: AudioMode,
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
    let invocation = parse_arguments(&arguments)?;
    let runtime = NdiRuntime::initialize()?;
    let finder = SourceFinder::new(&runtime, invocation.scope)?;
    match invocation.command {
        Command::ListSources => {
            list_sources(&finder, invocation.scope);
            Ok(())
        }
        Command::ShowVideo { query, options } => {
            show_video(&runtime, &finder, &query, invocation.scope, options)
        }
    }
}

fn parse_arguments(arguments: &[String]) -> Result<Invocation, String> {
    let sorted = sort_arguments(arguments)?;
    let command = parse_command(&sorted.names, &sorted.view_options)?;
    Ok(Invocation {
        scope: sorted.scope,
        command,
    })
}

/// `--ipaddr` takes the argument after it as its value; other options stand alone.
fn sort_arguments(arguments: &[String]) -> Result<SortedArguments<'_>, String> {
    let mut sorted = SortedArguments {
        names: Vec::new(),
        view_options: Vec::new(),
        scope: DiscoveryScope::Network,
    };
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        if argument == IP_ADDRESS_OPTION {
            if sorted.scope != DiscoveryScope::Network {
                return Err(USAGE.to_string());
            }
            sorted.scope = DiscoveryScope::Address(parse_ip_address(remaining.next())?);
        } else if argument.starts_with(OPTION_PREFIX) {
            sorted.view_options.push(argument);
        } else {
            sorted.names.push(argument);
        }
    }
    Ok(sorted)
}

fn parse_ip_address(value: Option<&String>) -> Result<IpAddr, String> {
    let value = value.ok_or_else(|| USAGE.to_string())?;
    value
        .parse()
        .map_err(|_| format!("\"{value}\" is not an IP address\n{USAGE}"))
}

fn parse_command(names: &[&String], view_options: &[&String]) -> Result<Command, String> {
    match names {
        [] if view_options.is_empty() => Ok(Command::ListSources),
        [device_name] => Ok(Command::ShowVideo {
            query: SourceQuery::any_stream_on(device_name.to_string()),
            options: parse_options(view_options)?,
        }),
        [device_name, stream_name] => Ok(Command::ShowVideo {
            query: SourceQuery::named_stream_on(device_name.to_string(), stream_name.to_string()),
            options: parse_options(view_options)?,
        }),
        _ => Err(USAGE.to_string()),
    }
}

fn parse_options(options: &[&String]) -> Result<ViewOptions, String> {
    let distinct: HashSet<&&String> = options.iter().collect();
    if distinct.len() != options.len() {
        return Err(USAGE.to_string());
    }
    let mut parsed = ViewOptions {
        window_mode: WindowMode::Windowed,
        audio_mode: AudioMode::Play,
    };
    for option in options {
        match option.as_str() {
            FULLSCREEN_OPTION => parsed.window_mode = WindowMode::Fullscreen,
            NO_AUDIO_OPTION => parsed.audio_mode = AudioMode::Mute,
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(parsed)
}

fn list_sources(finder: &SourceFinder, scope: DiscoveryScope) {
    let location = scope.location_suffix();
    println!("Discovering NDI sources{location} for {LISTING_DURATION:?}...");
    let names = finder.source_names_after(LISTING_DURATION);
    if names.is_empty() {
        println!("No NDI sources were discovered{location}");
    }
    for name in names {
        println!("{name}");
    }
}

fn show_video(
    runtime: &NdiRuntime,
    finder: &SourceFinder,
    query: &SourceQuery,
    scope: DiscoveryScope,
    options: ViewOptions,
) -> Result<(), String> {
    println!("Searching for {query}{}...", scope.location_suffix());
    let source = finder.find_source(query, DISCOVERY_TIMEOUT)?;
    println!("Connecting to \"{}\"", source.name());

    let receiver = Receiver::connect(runtime, &source)?;
    let sdl = sdl2::init()?;
    let mut window = VideoWindow::open(&sdl, &source.name(), options.window_mode)?;
    match options.audio_mode {
        AudioMode::Play => stream_video_and_audio(&sdl, &receiver, &mut window),
        AudioMode::Mute => stream_video(&receiver, &mut window, None),
    }
}

/// Plays the audio alongside the video; without a usable audio output, the video still shows.
fn stream_video_and_audio(
    sdl: &sdl2::Sdl,
    receiver: &Receiver,
    window: &mut VideoWindow,
) -> Result<(), String> {
    let buffer = Arc::new(Mutex::new(JitterBuffer::new(AUDIO_LATENCY, AUDIO_FADE)));
    match Speaker::new(sdl, Arc::clone(&buffer)) {
        Ok(mut speaker) => audio_pump::run_alongside(receiver, &buffer, || {
            stream_video(receiver, window, Some(&mut speaker))
        }),
        Err(error) => {
            report_audio_failure(&error);
            stream_video(receiver, window, None)
        }
    }
}

fn stream_video(
    receiver: &Receiver,
    window: &mut VideoWindow,
    mut speaker: Option<&mut Speaker>,
) -> Result<(), String> {
    let mut frame_rate = FrameRateMeter::new(FRAME_RATE_REPORT_INTERVAL);
    let mut stall = StallDetector::new(STALL_THRESHOLD);
    while window.handle_events()? == WindowStatus::Open {
        if let Some(speaker) = speaker.as_deref_mut() {
            follow_audio_format(speaker);
            report_audio_latency(speaker);
        }
        match receiver.capture_video(CAPTURE_TIMEOUT) {
            Capture::Frame(frame) => {
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

fn follow_audio_format(speaker: &mut Speaker) {
    match speaker.follow_source() {
        Ok(Some(format)) => println!("Playing audio: {format}"),
        Ok(None) => {}
        Err(error) => report_audio_failure(&error),
    }
}

fn report_audio_latency(speaker: &mut Speaker) {
    if let Some(latency) = speaker.raised_latency() {
        println!("Audio ran dry, now buffering {} ms", latency.as_millis());
    }
}

fn report_audio_failure(error: &str) {
    eprintln!("ndiclient: cannot play audio, continuing without it: {error}");
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

    fn command(values: &[&str]) -> Result<Command, String> {
        parse_arguments(&arguments(values)).map(|invocation| invocation.command)
    }

    fn scope(values: &[&str]) -> Option<DiscoveryScope> {
        parse_arguments(&arguments(values))
            .ok()
            .map(|invocation| invocation.scope)
    }

    fn address(text: &str) -> DiscoveryScope {
        DiscoveryScope::Address(text.parse().unwrap())
    }

    #[test]
    fn no_arguments_lists_sources() {
        let command = command(&[]);
        assert!(matches!(command, Ok(Command::ListSources)));
    }

    #[test]
    fn device_and_stream_names_show_video() {
        let command = command(&["STUDIO", "OBS PGM"]);
        assert!(matches!(command, Ok(Command::ShowVideo { .. })));
    }

    #[test]
    fn device_name_alone_shows_video() {
        let command = command(&["STUDIO"]);
        assert!(matches!(command, Ok(Command::ShowVideo { .. })));
    }

    fn options(values: &[&str]) -> Option<ViewOptions> {
        match command(values) {
            Ok(Command::ShowVideo { options, .. }) => Some(options),
            _ => None,
        }
    }

    fn window_mode(values: &[&str]) -> Option<WindowMode> {
        options(values).map(|options| options.window_mode)
    }

    fn audio_mode(values: &[&str]) -> Option<AudioMode> {
        options(values).map(|options| options.audio_mode)
    }

    #[test]
    fn video_is_windowed_by_default() {
        assert_eq!(window_mode(&["STUDIO"]), Some(WindowMode::Windowed));
    }

    #[test]
    fn fullscreen_option_shows_video_fullscreen() {
        let mode = window_mode(&["--fullscreen", "STUDIO", "OBS PGM"]);
        assert_eq!(mode, Some(WindowMode::Fullscreen));
    }

    #[test]
    fn fullscreen_option_may_follow_names() {
        let mode = window_mode(&["STUDIO", "--fullscreen"]);
        assert_eq!(mode, Some(WindowMode::Fullscreen));
    }

    #[test]
    fn fullscreen_option_without_device_is_rejected() {
        assert!(command(&["--fullscreen"]).is_err());
    }

    #[test]
    fn audio_plays_by_default() {
        assert_eq!(audio_mode(&["STUDIO"]), Some(AudioMode::Play));
    }

    #[test]
    fn noaudio_option_mutes_audio() {
        let mode = audio_mode(&["--noaudio", "STUDIO", "OBS PGM"]);
        assert_eq!(mode, Some(AudioMode::Mute));
    }

    #[test]
    fn noaudio_option_keeps_window_mode() {
        let mode = window_mode(&["STUDIO", "--noaudio"]);
        assert_eq!(mode, Some(WindowMode::Windowed));
    }

    #[test]
    fn options_combine_in_any_order() {
        let parsed = options(&["--noaudio", "STUDIO", "--fullscreen"]);
        let expected = ViewOptions {
            window_mode: WindowMode::Fullscreen,
            audio_mode: AudioMode::Mute,
        };
        assert_eq!(parsed, Some(expected));
    }

    #[test]
    fn noaudio_option_without_device_is_rejected() {
        assert!(command(&["--noaudio"]).is_err());
    }

    #[test]
    fn repeated_noaudio_option_is_rejected() {
        let command = command(&["--noaudio", "STUDIO", "--noaudio"]);
        assert!(command.is_err());
    }

    #[test]
    fn unknown_option_is_rejected() {
        assert!(command(&["--kiosk", "STUDIO"]).is_err());
    }

    #[test]
    fn repeated_option_is_rejected() {
        let command = command(&["--fullscreen", "--fullscreen", "STUDIO"]);
        assert!(command.is_err());
    }

    #[test]
    fn extra_argument_is_rejected() {
        assert!(command(&["STUDIO", "OBS PGM", "extra"]).is_err());
    }

    #[test]
    fn sources_are_discovered_on_the_network_by_default() {
        assert_eq!(scope(&["STUDIO"]), Some(DiscoveryScope::Network));
    }

    #[test]
    fn ipaddr_option_restricts_discovery_to_address() {
        let parsed = scope(&["--ipaddr", "192.0.2.20", "STUDIO", "OBS PGM"]);
        assert_eq!(parsed, Some(address("192.0.2.20")));
    }

    #[test]
    fn ipaddr_option_may_follow_names() {
        let parsed = scope(&["STUDIO", "--ipaddr", "192.0.2.20"]);
        assert_eq!(parsed, Some(address("192.0.2.20")));
    }

    #[test]
    fn ipaddr_option_accepts_ipv6_address() {
        assert_eq!(
            scope(&["--ipaddr", "2001:db8::20", "STUDIO"]),
            Some(address("2001:db8::20"))
        );
    }

    #[test]
    fn ipaddr_value_is_not_taken_as_device_name() {
        let command = command(&["--ipaddr", "192.0.2.20", "STUDIO", "OBS PGM"]);
        assert!(matches!(command, Ok(Command::ShowVideo { .. })));
    }

    #[test]
    fn ipaddr_option_alone_lists_sources() {
        let command = command(&["--ipaddr", "192.0.2.20"]);
        assert!(matches!(command, Ok(Command::ListSources)));
    }

    #[test]
    fn ipaddr_option_combines_with_view_options() {
        let mode = window_mode(&["--fullscreen", "--ipaddr", "192.0.2.20", "STUDIO"]);
        assert_eq!(mode, Some(WindowMode::Fullscreen));
    }

    #[test]
    fn ipaddr_option_without_value_is_rejected() {
        assert!(command(&["STUDIO", "--ipaddr"]).is_err());
    }

    #[test]
    fn ipaddr_option_with_host_name_is_rejected() {
        assert!(command(&["--ipaddr", "studio.local", "STUDIO"]).is_err());
    }

    #[test]
    fn repeated_ipaddr_option_is_rejected() {
        let command = command(&["--ipaddr", "198.51.100.1", "--ipaddr", "198.51.100.2", "STUDIO"]);
        assert!(command.is_err());
    }
}
