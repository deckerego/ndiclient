//! NDI client: discovers the NDI source with a given device name (and
//! optionally stream name), shows its video in a desktop window or
//! fullscreen and plays its audio. Without arguments it lists the NDI
//! sources it discovers. `--ipaddr` restricts both to one IP address.
//! `--wait-image` shows a BMP image while the source sends no video, and
//! `--retry-wait` connects anew when no video comes for too long.

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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use display::{VideoWindow, WindowMode, WindowStatus};
use frame_rate::FrameRateMeter;
use jitter_buffer::{AudioLatency, JitterBuffer};
use ndi::{
    DiscoveryScope, FrameStamp, FrameSync, NdiRuntime, Receiver, SourceDescriptor, SourceFinder,
    SourceQuery,
};
use speaker::Speaker;
use stall::{RetryPolicy, StallDetector};
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
const RETRY_WAIT_OPTION: &str = "--retry-wait";
/// Options that take the argument after them as their value.
const VALUE_OPTIONS: [&str; 3] = [IP_ADDRESS_OPTION, WAIT_IMAGE_OPTION, RETRY_WAIT_OPTION];
const OPTION_PREFIX: &str = "-";
const USAGE: &str =
    "usage: ndiclient [-d] [--fullscreen] [--noaudio] [--ipaddr <ip>] [--wait-image <file.bmp>]
                 [--retry-wait <seconds>] <device-name> [stream-name]
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
    flags: Vec<&'a String>,
    /// The value given to each of the `VALUE_OPTIONS` used.
    values: HashMap<&'a str, &'a String>,
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
    /// Whether a long stall makes the viewer connect anew.
    retry: RetryPolicy,
}

/// Why watching one connection came to an end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    WindowClosed,
    Reconnect,
}

/// What a search for the source with the window already open came to.
enum Rediscovery {
    Found(SourceDescriptor),
    Missing(String),
    WindowClosed,
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
    match invocation.command {
        Command::ListSources => list_sources(&runtime, invocation.scope),
        Command::ShowVideo { query, options } => {
            show_video(&runtime, &query, invocation.scope, options)
        }
    }
}

fn parse_arguments(arguments: &[String]) -> Result<Invocation, String> {
    let sorted = sort_arguments(arguments)?;
    let scope = parse_scope(sorted.values.get(IP_ADDRESS_OPTION).copied())?;
    let command = parse_command(&sorted)?;
    Ok(Invocation { scope, command })
}

/// The `VALUE_OPTIONS` take the argument after them as their value; other
/// options stand alone. Each of them may be given once.
fn sort_arguments(arguments: &[String]) -> Result<SortedArguments<'_>, String> {
    let mut sorted = SortedArguments {
        names: Vec::new(),
        flags: Vec::new(),
        values: HashMap::new(),
    };
    let mut remaining = arguments.iter();
    while let Some(argument) = remaining.next() {
        if VALUE_OPTIONS.contains(&argument.as_str()) {
            let value = remaining.next().ok_or_else(|| USAGE.to_string())?;
            if sorted.values.insert(argument.as_str(), value).is_some() {
                return Err(USAGE.to_string());
            }
        } else if argument.starts_with(OPTION_PREFIX) {
            sorted.flags.push(argument);
        } else {
            sorted.names.push(argument);
        }
    }
    Ok(sorted)
}

fn parse_scope(ip_address: Option<&String>) -> Result<DiscoveryScope, String> {
    let Some(value) = ip_address else {
        return Ok(DiscoveryScope::Network);
    };
    value
        .parse()
        .map(DiscoveryScope::Address)
        .map_err(|_| format!("\"{value}\" is not an IP address\n{USAGE}"))
}

fn parse_command(sorted: &SortedArguments) -> Result<Command, String> {
    let lists_sources = sorted.flags.is_empty()
        && sorted
            .values
            .keys()
            .all(|option| *option == IP_ADDRESS_OPTION);
    let query = match sorted.names.as_slice() {
        [] if lists_sources => return Ok(Command::ListSources),
        [device_name] => SourceQuery::any_stream_on(device_name.to_string()),
        [device_name, stream_name] => {
            SourceQuery::named_stream_on(device_name.to_string(), stream_name.to_string())
        }
        _ => return Err(USAGE.to_string()),
    };
    let options = parse_options(sorted)?;
    Ok(Command::ShowVideo { query, options })
}

fn parse_options(sorted: &SortedArguments) -> Result<ViewOptions, String> {
    let flags = &sorted.flags;
    let distinct: HashSet<&&String> = flags.iter().collect();
    if distinct.len() != flags.len() {
        return Err(USAGE.to_string());
    }
    let mut parsed = ViewOptions {
        window_mode: WindowMode::Windowed,
        audio_mode: AudioMode::Play,
        verbosity: Verbosity::Quiet,
        wait_image: sorted
            .values
            .get(WAIT_IMAGE_OPTION)
            .copied()
            .map(PathBuf::from),
        retry: parse_retry_wait(sorted.values.get(RETRY_WAIT_OPTION).copied())?,
    };
    for flag in flags {
        match flag.as_str() {
            FULLSCREEN_OPTION => parsed.window_mode = WindowMode::Fullscreen,
            NO_AUDIO_OPTION => parsed.audio_mode = AudioMode::Mute,
            DEBUG_OPTION => parsed.verbosity = Verbosity::Debug,
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(parsed)
}

/// Without `--retry-wait`, a stalled connection is waited for indefinitely.
fn parse_retry_wait(seconds: Option<&String>) -> Result<RetryPolicy, String> {
    let Some(value) = seconds else {
        return Ok(RetryPolicy::KeepWaiting);
    };
    value
        .parse()
        .map(|seconds| RetryPolicy::ReconnectAfter(Duration::from_secs(seconds)))
        .map_err(|_| format!("\"{value}\" is not a whole number of seconds\n{USAGE}"))
}

fn list_sources(runtime: &NdiRuntime, scope: DiscoveryScope) -> Result<(), String> {
    let finder = SourceFinder::new(runtime, scope)?;
    let location = scope.location_suffix();
    println!("Discovering NDI sources{location} for {LISTING_DURATION:?}...");
    let names = finder.source_names_after(LISTING_DURATION);
    if names.is_empty() {
        println!("No NDI sources were discovered{location}");
    }
    for name in names {
        println!("{name}");
    }
    Ok(())
}

/// A source that isn't found at start-up is an error. Once the window is
/// open, reconnections search again until the source is back.
fn show_video(
    runtime: &NdiRuntime,
    query: &SourceQuery,
    scope: DiscoveryScope,
    options: ViewOptions,
) -> Result<(), String> {
    let wait_image = load_wait_image(options.wait_image.as_deref())?;
    let finder = SourceFinder::new(runtime, scope)?;
    announce_search(query, scope);
    let source = finder.find_source(query, DISCOVERY_TIMEOUT)?;
    let sdl = sdl2::init()?;
    let window = VideoWindow::open(&sdl, &source.name(), options.window_mode, wait_image)?;
    let mut viewer = Viewer {
        sdl,
        window,
        options,
    };
    let mut ending = viewer.watch(runtime, &source)?;
    // Each reconnection starts from scratch, with a finder of its own.
    drop(finder);
    while ending == Ending::Reconnect {
        ending = viewer.reconnect(runtime, query, scope)?;
    }
    Ok(())
}

fn load_wait_image(path: Option<&Path>) -> Result<Option<WaitImage>, String> {
    path.map(WaitImage::load).transpose()
}

fn announce_search(query: &SourceQuery, scope: DiscoveryScope) {
    println!("Searching for {query}{}...", scope.location_suffix());
}

/// The frame rate is a debug message, so it is only measured when debugging.
fn frame_rate_meter(verbosity: Verbosity) -> Option<FrameRateMeter> {
    match verbosity {
        Verbosity::Debug => Some(FrameRateMeter::new(FRAME_RATE_REPORT_INTERVAL)),
        Verbosity::Quiet => None,
    }
}

/// The window and how to play the source in it, kept across reconnections.
struct Viewer {
    sdl: sdl2::Sdl,
    window: VideoWindow,
    options: ViewOptions,
}

impl Viewer {
    /// Connects to `source` and plays it until the window is closed or the
    /// retry policy gives up on the connection. All NDI objects of the
    /// connection are gone when it returns.
    fn watch(&mut self, runtime: &NdiRuntime, source: &SourceDescriptor) -> Result<Ending, String> {
        println!("Connecting to \"{}\"", source.name());
        let receiver = Receiver::connect(runtime, source)?;
        let frame_sync = FrameSync::new(&receiver)?;
        match self.options.audio_mode {
            AudioMode::Play => self.stream_video_and_audio(&frame_sync),
            AudioMode::Mute => self.stream_video(&frame_sync, None),
        }
    }

    /// Searches for the source and connects to it as at start-up, while the
    /// window keeps showing the last picture or the wait image. A search
    /// that finds nothing is reported, and the caller tries again.
    fn reconnect(
        &mut self,
        runtime: &NdiRuntime,
        query: &SourceQuery,
        scope: DiscoveryScope,
    ) -> Result<Ending, String> {
        println!("Reconnecting");
        let finder = SourceFinder::new(runtime, scope)?;
        announce_search(query, scope);
        match self.rediscover(&finder, query)? {
            Rediscovery::Found(source) => self.watch(runtime, &source),
            Rediscovery::Missing(message) => {
                eprintln!("ndiclient: {message}");
                Ok(Ending::Reconnect)
            }
            Rediscovery::WindowClosed => Ok(Ending::WindowClosed),
        }
    }

    /// Polls the search once per display refresh, so the window still
    /// answers to its keys and can be closed meanwhile.
    fn rediscover(
        &mut self,
        finder: &SourceFinder,
        query: &SourceQuery,
    ) -> Result<Rediscovery, String> {
        let search = finder.search(query, DISCOVERY_TIMEOUT);
        while self.window.handle_events()? == WindowStatus::Open {
            match search.poll() {
                Ok(Some(source)) => return Ok(Rediscovery::Found(source)),
                Ok(None) => self.window.present()?,
                Err(message) => return Ok(Rediscovery::Missing(message)),
            }
        }
        Ok(Rediscovery::WindowClosed)
    }

    /// Plays the audio alongside the video; without a usable audio output,
    /// the video still shows.
    fn stream_video_and_audio(&mut self, frame_sync: &FrameSync) -> Result<Ending, String> {
        let buffer = Arc::new(Mutex::new(JitterBuffer::new(AUDIO_LATENCY, AUDIO_FADE)));
        match Speaker::new(&self.sdl, Arc::clone(&buffer)) {
            Ok(mut speaker) => audio_pump::run_alongside(frame_sync, &buffer, || {
                self.stream_video(frame_sync, Some(&mut speaker))
            }),
            Err(error) => {
                report_audio_failure(&error);
                self.stream_video(frame_sync, None)
            }
        }
    }

    /// Draws once per display refresh, paced by vsync: a frame the source
    /// has newly sent is uploaded, otherwise the last picture is drawn again.
    fn stream_video(
        &mut self,
        frame_sync: &FrameSync,
        mut speaker: Option<&mut Speaker>,
    ) -> Result<Ending, String> {
        let mut stall = StallDetector::new(STALL_THRESHOLD);
        let mut frame_rate = frame_rate_meter(self.options.verbosity);
        let mut shown: Option<FrameStamp> = None;
        while self.window.handle_events()? == WindowStatus::Open {
            if let Some(speaker) = speaker.as_deref_mut() {
                follow_audio_format(speaker);
                report_audio_latency(speaker);
            }
            let frame = frame_sync.capture_video();
            let stamp = frame.as_ref().map(ndi::VideoFrame::stamp);
            match frame {
                Some(frame) if stamp != shown => {
                    show_frame(&frame, &mut self.window)?;
                    shown = stamp;
                    stall.record_frame();
                    if let Some(frame_rate) = frame_rate.as_mut() {
                        report_frame_rate(frame_rate);
                    }
                }
                _ => {
                    if stall.take_new_stall() {
                        report_stall(&mut self.window)?;
                    }
                    if self.options.retry.gives_up_on(&stall) {
                        return Ok(Ending::Reconnect);
                    }
                    self.window.present()?;
                }
            }
        }
        Ok(Ending::WindowClosed)
    }
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
            retry: RetryPolicy::KeepWaiting,
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

    fn retry(values: &[&str]) -> Option<RetryPolicy> {
        options(values).map(|options| options.retry)
    }

    fn reconnect_after(seconds: u64) -> Option<RetryPolicy> {
        Some(RetryPolicy::ReconnectAfter(Duration::from_secs(seconds)))
    }

    #[test]
    fn stalled_video_is_waited_for_by_default() {
        assert_eq!(retry(&["STUDIO"]), Some(RetryPolicy::KeepWaiting));
    }

    #[test]
    fn retry_wait_option_reconnects_after_seconds() {
        let parsed = retry(&["--retry-wait", "30", "STUDIO", "OBS PGM"]);
        assert_eq!(parsed, reconnect_after(30));
    }

    #[test]
    fn retry_wait_option_may_follow_names() {
        assert_eq!(
            retry(&["STUDIO", "--retry-wait", "30"]),
            reconnect_after(30)
        );
    }

    #[test]
    fn retry_wait_option_accepts_zero() {
        assert_eq!(retry(&["--retry-wait", "0", "STUDIO"]), reconnect_after(0));
    }

    #[test]
    fn retry_wait_value_is_not_taken_as_stream_name() {
        let command = command(&["STUDIO", "--retry-wait", "30", "extra", "more"]);
        assert!(command.is_err());
    }

    #[test]
    fn retry_wait_option_combines_with_value_options() {
        let parsed = retry(&[
            "--ipaddr",
            "192.0.2.20",
            "--retry-wait",
            "30",
            "--wait-image",
            "standby.bmp",
            "STUDIO",
        ]);
        assert_eq!(parsed, reconnect_after(30));
    }

    #[test]
    fn retry_wait_option_without_device_is_rejected() {
        assert!(command(&["--retry-wait", "30"]).is_err());
    }

    #[test]
    fn retry_wait_option_with_ipaddr_but_without_device_is_rejected() {
        assert!(command(&["--ipaddr", "192.0.2.20", "--retry-wait", "30"]).is_err());
    }

    #[test]
    fn retry_wait_option_without_seconds_is_rejected() {
        assert!(command(&["STUDIO", "--retry-wait"]).is_err());
    }

    #[test]
    fn retry_wait_option_with_word_is_rejected() {
        assert!(command(&["--retry-wait", "soon", "STUDIO"]).is_err());
    }

    #[test]
    fn retry_wait_option_with_negative_seconds_is_rejected() {
        assert!(command(&["--retry-wait", "-5", "STUDIO"]).is_err());
    }

    #[test]
    fn retry_wait_option_with_fractional_seconds_is_rejected() {
        assert!(command(&["--retry-wait", "1.5", "STUDIO"]).is_err());
    }

    #[test]
    fn repeated_retry_wait_option_is_rejected() {
        let command = command(&["--retry-wait", "10", "--retry-wait", "20", "STUDIO"]);
        assert!(command.is_err());
    }
}
