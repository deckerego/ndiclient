//! NDI client: discovers the NDI source with a given device name (and
//! optionally stream name), shows its video in a desktop window or
//! fullscreen and plays its audio. Without arguments it lists the NDI
//! sources it discovers. `--ipaddr` restricts both to one IP address.
//! `--wait-image` shows a BMP image while the source sends no video.

mod audio_pump;
mod display;
mod frame_rate;
mod i420;
mod jitter_buffer;
mod ndi;
mod padding;
mod picture;
mod sound;
mod speaker;
mod stall;
mod wait_image;

use std::collections::HashSet;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use display::{VideoWindow, WindowMode, WindowStatus};
use frame_rate::FrameRateMeter;
use jitter_buffer::{AudioLatency, JitterBuffer};
use ndi::{DiscoveryScope, FrameStamp, FrameSync, NdiRuntime, Receiver, SourceFinder, SourceQuery};
use speaker::Speaker;
use stall::StallDetector;
use wait_image::WaitImage;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for sources on a typical LAN to answer mDNS discovery.
const LISTING_DURATION: Duration = Duration::from_secs(5);
const FRAME_RATE_REPORT_INTERVAL: Duration = Duration::from_secs(5);
const STALL_THRESHOLD: Duration = Duration::from_secs(3);
/// Audio held in reserve on top of the frame-sync's own queue. Unbroken
/// audio matters more than closeness to the video, so it starts beyond the
/// arrival gaps seen on a loaded Wi-Fi and underruns raise it further.
const AUDIO_LATENCY: AudioLatency = AudioLatency {
    start: Duration::from_millis(100),
    limit: Duration::from_millis(400),
};
/// Short enough not to be heard as a fade, long enough not to click.
const AUDIO_FADE: Duration = Duration::from_millis(5);
const FULLSCREEN_OPTION: &str = "--fullscreen";
const NO_AUDIO_OPTION: &str = "--noaudio";
const DEBUG_OPTION: &str = "-d";
const IP_ADDRESS_OPTION: &str = "--ipaddr";
const WAIT_IMAGE_OPTION: &str = "--wait-image";
const OPTION_PREFIX: &str = "-";
const USAGE: &str =
    "usage: ndiclient [-d] [--fullscreen] [--noaudio] [--ipaddr <ip>] [--wait-image <file.bmp>]
                 <device-name> [stream-name]
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
    wait_image: Option<PathBuf>,
    scope: DiscoveryScope,
}

/// Whether the source's audio is played. Without it, no audio device is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AudioMode {
    Play,
    Mute,
}

/// Whether debug messages (such as the displayed frame rate) are printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verbosity {
    Quiet,
    Debug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ViewOptions {
    window_mode: WindowMode,
    audio_mode: AudioMode,
    verbosity: Verbosity,
    /// Shown instead of the video while the source sends none.
    wait_image: Option<PathBuf>,
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
    let scope = sorted.scope;
    let command = parse_command(sorted)?;
    Ok(Invocation { scope, command })
}

/// `--ipaddr` and `--wait-image` take the argument after them as their
/// value; other options stand alone.
fn sort_arguments(arguments: &[String]) -> Result<SortedArguments<'_>, String> {
    let mut sorted = SortedArguments {
        names: Vec::new(),
        view_options: Vec::new(),
        wait_image: None,
        scope: DiscoveryScope::Network,
    };
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        if argument == IP_ADDRESS_OPTION {
            if sorted.scope != DiscoveryScope::Network {
                return Err(USAGE.to_string());
            }
            sorted.scope = DiscoveryScope::Address(parse_ip_address(remaining.next())?);
        } else if argument == WAIT_IMAGE_OPTION {
            if sorted.wait_image.is_some() {
                return Err(USAGE.to_string());
            }
            let path = remaining.next().ok_or_else(|| USAGE.to_string())?;
            sorted.wait_image = Some(PathBuf::from(path));
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

fn parse_command(sorted: SortedArguments) -> Result<Command, String> {
    let lists_sources = sorted.view_options.is_empty() && sorted.wait_image.is_none();
    let query = match sorted.names.as_slice() {
        [] if lists_sources => return Ok(Command::ListSources),
        [device_name] => SourceQuery::any_stream_on(device_name.to_string()),
        [device_name, stream_name] => {
            SourceQuery::named_stream_on(device_name.to_string(), stream_name.to_string())
        }
        _ => return Err(USAGE.to_string()),
    };
    let options = parse_options(&sorted.view_options, sorted.wait_image)?;
    Ok(Command::ShowVideo { query, options })
}

fn parse_options(options: &[&String], wait_image: Option<PathBuf>) -> Result<ViewOptions, String> {
    let distinct: HashSet<&&String> = options.iter().collect();
    if distinct.len() != options.len() {
        return Err(USAGE.to_string());
    }
    let mut parsed = ViewOptions {
        window_mode: WindowMode::Windowed,
        audio_mode: AudioMode::Play,
        verbosity: Verbosity::Quiet,
        wait_image,
    };
    for option in options {
        match option.as_str() {
            FULLSCREEN_OPTION => parsed.window_mode = WindowMode::Fullscreen,
            NO_AUDIO_OPTION => parsed.audio_mode = AudioMode::Mute,
            DEBUG_OPTION => parsed.verbosity = Verbosity::Debug,
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
    let wait_image = load_wait_image(options.wait_image)?;
    println!("Searching for {query}{}...", scope.location_suffix());
    let source = finder.find_source(query, DISCOVERY_TIMEOUT)?;
    println!("Connecting to \"{}\"", source.name());

    let receiver = Receiver::connect(runtime, &source)?;
    let frame_sync = FrameSync::new(&receiver)?;
    let sdl = sdl2::init()?;
    let mut window = VideoWindow::open(&sdl, &source.name(), options.window_mode, wait_image)?;
    let frame_rate = frame_rate_meter(options.verbosity);
    match options.audio_mode {
        AudioMode::Play => stream_video_and_audio(&sdl, &frame_sync, &mut window, frame_rate),
        AudioMode::Mute => stream_video(&frame_sync, &mut window, None, frame_rate),
    }
}

fn load_wait_image(path: Option<PathBuf>) -> Result<Option<WaitImage>, String> {
    path.map(|path| WaitImage::load(&path)).transpose()
}

/// The frame rate is a debug message, so it is only measured when debugging.
fn frame_rate_meter(verbosity: Verbosity) -> Option<FrameRateMeter> {
    match verbosity {
        Verbosity::Debug => Some(FrameRateMeter::new(FRAME_RATE_REPORT_INTERVAL)),
        Verbosity::Quiet => None,
    }
}

/// Plays the audio alongside the video; without a usable audio output, the video still shows.
fn stream_video_and_audio(
    sdl: &sdl2::Sdl,
    frame_sync: &FrameSync,
    window: &mut VideoWindow,
    frame_rate: Option<FrameRateMeter>,
) -> Result<(), String> {
    let buffer = Arc::new(Mutex::new(JitterBuffer::new(AUDIO_LATENCY, AUDIO_FADE)));
    match Speaker::new(sdl, Arc::clone(&buffer)) {
        Ok(mut speaker) => audio_pump::run_alongside(frame_sync, &buffer, || {
            stream_video(frame_sync, window, Some(&mut speaker), frame_rate)
        }),
        Err(error) => {
            report_audio_failure(&error);
            stream_video(frame_sync, window, None, frame_rate)
        }
    }
}

/// Draws once per display refresh, paced by vsync: a frame the source has
/// newly sent is uploaded, otherwise the last picture is drawn again.
fn stream_video(
    frame_sync: &FrameSync,
    window: &mut VideoWindow,
    mut speaker: Option<&mut Speaker>,
    mut frame_rate: Option<FrameRateMeter>,
) -> Result<(), String> {
    let mut stall = StallDetector::new(STALL_THRESHOLD);
    let mut shown: Option<FrameStamp> = None;
    while window.handle_events()? == WindowStatus::Open {
        if let Some(speaker) = speaker.as_deref_mut() {
            follow_audio_format(speaker);
            report_audio_latency(speaker);
        }
        let frame = frame_sync.capture_video();
        let stamp = frame.as_ref().map(ndi::VideoFrame::stamp);
        match frame {
            Some(frame) if stamp != shown => {
                show_frame(&frame, window)?;
                shown = stamp;
                stall.record_frame();
                if let Some(frame_rate) = frame_rate.as_mut() {
                    report_frame_rate(frame_rate);
                }
            }
            _ => {
                if stall.take_new_stall() {
                    report_stall(window)?;
                }
                window.present()?;
            }
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

/// Shows the wait image (if any) until video returns, besides telling the console.
fn report_stall(window: &mut VideoWindow) -> Result<(), String> {
    println!("Waiting for video...");
    window.show_wait_image()
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
            verbosity: Verbosity::Quiet,
            wait_image: None,
        };
        assert_eq!(parsed, Some(expected));
    }

    fn verbosity(values: &[&str]) -> Option<Verbosity> {
        options(values).map(|options| options.verbosity)
    }

    #[test]
    fn debug_messages_are_suppressed_by_default() {
        assert_eq!(verbosity(&["STUDIO"]), Some(Verbosity::Quiet));
    }

    #[test]
    fn debug_option_prints_debug_messages() {
        let parsed = verbosity(&["-d", "STUDIO", "OBS PGM"]);
        assert_eq!(parsed, Some(Verbosity::Debug));
    }

    #[test]
    fn debug_option_may_follow_names() {
        assert_eq!(verbosity(&["STUDIO", "-d"]), Some(Verbosity::Debug));
    }

    #[test]
    fn debug_option_without_device_is_rejected() {
        assert!(command(&["-d"]).is_err());
    }

    #[test]
    fn repeated_debug_option_is_rejected() {
        assert!(command(&["-d", "STUDIO", "-d"]).is_err());
    }

    #[test]
    fn unknown_short_option_is_rejected() {
        assert!(command(&["-x", "STUDIO"]).is_err());
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
        let command = command(&[
            "--ipaddr",
            "198.51.100.1",
            "--ipaddr",
            "198.51.100.2",
            "STUDIO",
        ]);
        assert!(command.is_err());
    }

    fn wait_image(values: &[&str]) -> Option<Option<PathBuf>> {
        options(values).map(|options| options.wait_image)
    }

    #[test]
    fn no_wait_image_by_default() {
        assert_eq!(wait_image(&["STUDIO"]), Some(None));
    }

    #[test]
    fn wait_image_option_takes_path() {
        let parsed = wait_image(&["--wait-image", "standby.bmp", "STUDIO", "OBS PGM"]);
        assert_eq!(parsed, Some(Some(PathBuf::from("standby.bmp"))));
    }

    #[test]
    fn wait_image_option_may_follow_names() {
        let parsed = wait_image(&["STUDIO", "--wait-image", "standby.bmp"]);
        assert_eq!(parsed, Some(Some(PathBuf::from("standby.bmp"))));
    }

    #[test]
    fn wait_image_path_is_not_taken_as_stream_name() {
        let command = command(&["STUDIO", "--wait-image", "standby.bmp", "extra", "more"]);
        assert!(command.is_err());
    }

    #[test]
    fn wait_image_option_combines_with_view_options() {
        let mode = window_mode(&["--fullscreen", "--wait-image", "standby.bmp", "STUDIO"]);
        assert_eq!(mode, Some(WindowMode::Fullscreen));
    }

    #[test]
    fn wait_image_option_without_device_is_rejected() {
        assert!(command(&["--wait-image", "standby.bmp"]).is_err());
    }

    #[test]
    fn wait_image_option_without_path_is_rejected() {
        assert!(command(&["STUDIO", "--wait-image"]).is_err());
    }

    #[test]
    fn repeated_wait_image_option_is_rejected() {
        let command = command(&[
            "--wait-image",
            "standby.bmp",
            "--wait-image",
            "offline.bmp",
            "STUDIO",
        ]);
        assert!(command.is_err());
    }
}
