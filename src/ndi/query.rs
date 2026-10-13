use std::fmt;

/// Which NDI source to connect to, named by the two parts of its full NDI
/// name `DEVICE (Stream)`: any stream on a device, or one stream by name.
pub struct SourceQuery {
    device_name: String,
    stream_name: Option<String>,
}

impl SourceQuery {
    pub fn any_stream_on(device_name: String) -> Self {
        Self {
            device_name,
            stream_name: None,
        }
    }

    pub fn named_stream_on(device_name: String, stream_name: String) -> Self {
        Self {
            device_name,
            stream_name: Some(stream_name),
        }
    }

    /// Device names are host names, so they match case-insensitively; stream
    /// names must match exactly.
    pub fn matches(&self, full_name: &str) -> bool {
        split_full_name(full_name).is_some_and(|(device_name, stream_name)| {
            device_name.eq_ignore_ascii_case(&self.device_name) && self.matches_stream(stream_name)
        })
    }

    fn matches_stream(&self, stream_name: &str) -> bool {
        self.stream_name
            .as_deref()
            .is_none_or(|wanted| stream_name == wanted)
    }
}

impl fmt::Display for SourceQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let device_name = &self.device_name;
        match &self.stream_name {
            Some(stream_name) => write!(
                formatter,
                "NDI stream \"{stream_name}\" on device \"{device_name}\""
            ),
            None => write!(formatter, "an NDI stream on device \"{device_name}\""),
        }
    }
}

/// NDI full names have the form `DEVICE (Stream Name)`.
fn split_full_name(full_name: &str) -> Option<(&str, &str)> {
    let (device_name, rest) = full_name.split_once(" (")?;
    let stream_name = rest.strip_suffix(')')?;
    Some((device_name, stream_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_NAME: &str = "STUDIO.LOCAL (OBS PGM)";

    fn query(device_name: &str, stream_name: &str) -> SourceQuery {
        SourceQuery::named_stream_on(device_name.to_string(), stream_name.to_string())
    }

    fn any_stream_query(device_name: &str) -> SourceQuery {
        SourceQuery::any_stream_on(device_name.to_string())
    }

    #[test]
    fn any_stream_query_matches_stream_on_device() {
        assert!(any_stream_query("STUDIO.LOCAL").matches(FULL_NAME));
    }

    #[test]
    fn any_stream_query_matches_device_ignoring_case() {
        assert!(any_stream_query("studio.local").matches(FULL_NAME));
    }

    #[test]
    fn any_stream_query_rejects_other_device() {
        assert!(!any_stream_query("BOOTH").matches(FULL_NAME));
    }

    #[test]
    fn matching_device_and_stream_match() {
        assert!(query("STUDIO.LOCAL", "OBS PGM").matches(FULL_NAME));
    }

    #[test]
    fn device_name_matches_ignoring_case() {
        assert!(query("studio.local", "OBS PGM").matches(FULL_NAME));
    }

    #[test]
    fn stream_name_is_case_sensitive() {
        assert!(!query("STUDIO.LOCAL", "obs pgm").matches(FULL_NAME));
    }

    #[test]
    fn other_device_does_not_match() {
        assert!(!query("BOOTH", "OBS PGM").matches(FULL_NAME));
    }

    #[test]
    fn other_stream_does_not_match() {
        assert!(!query("STUDIO.LOCAL", "Test Patterns").matches(FULL_NAME));
    }

    #[test]
    fn partial_stream_name_does_not_match() {
        assert!(!query("STUDIO.LOCAL", "OBS").matches(FULL_NAME));
    }

    #[test]
    fn name_without_stream_part_does_not_match() {
        assert!(!query("STUDIO.LOCAL", "OBS PGM").matches("STUDIO.LOCAL"));
    }

    #[test]
    fn split_keeps_parentheses_inside_stream_name() {
        assert_eq!(
            split_full_name("PC (Cam (wide))"),
            Some(("PC", "Cam (wide)"))
        );
    }
}
