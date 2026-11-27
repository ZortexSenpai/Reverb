//! HQPlayer's control protocol: each request and each reply is one small XML
//! document, newline-terminated, whose root element names the command, with
//! arguments and results as attributes — `<Seek position="42"/>`, answered
//! `<Seek result="OK"/>` or `<Seek result="Error">reason</Seek>`.
//!
//! The wire format follows Signalyst's own control client (`hqp-control`,
//! the reference for HQPlayer 4, 5 and 6 alike). This module only builds and
//! reads the documents; the connection is `client`'s.

use std::borrow::Cow;
use std::collections::HashMap;

use quick_xml::escape::escape;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde::Serialize;

const DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;

/// `<Name a="1" b="2"/>`, attribute values escaped.
fn element(name: &str, attributes: &[(&str, Cow<'_, str>)]) -> String {
    let mut xml = format!("{DECLARATION}<{name}");
    for (key, value) in attributes {
        xml.push_str(&format!(r#" {key}="{}""#, escape(value.as_ref())));
    }
    xml.push_str("/>");
    xml
}

fn bare(name: &str) -> String {
    element(name, &[])
}

// ---- requests -------------------------------------------------------------

pub fn get_info() -> String {
    bare("GetInfo")
}

/// A one-off status. `subscribe="0"` matters: a bare `<Status/>` subscribes
/// the connection to pushes, which a request/reply channel cannot take.
pub fn status() -> String {
    element("Status", &[("subscribe", "0".into())])
}

pub fn volume_range() -> String {
    bare("VolumeRange")
}

/// Start the playlist (from its first entry, on a stopped HQPlayer).
pub fn play() -> String {
    element("Play", &[("last", "0".into())])
}

/// Pause — or, on a paused HQPlayer, resume: it is a toggle.
pub fn pause() -> String {
    bare("Pause")
}

pub fn stop() -> String {
    bare("Stop")
}

/// Jump to `position_secs` into the playing Song; whole seconds only.
pub fn seek(position_secs: u64) -> String {
    element("Seek", &[("position", position_secs.to_string().into())])
}

/// Set the volume, in dB (absolute, within `VolumeRange`).
pub fn set_volume(db: f32) -> String {
    element("Volume", &[("value", format!("{db:.1}").into())])
}

/// Empty the playlist. A playing or paused entry survives it, so stop first.
pub fn playlist_clear() -> String {
    bare("PlaylistClear")
}

/// Take the entry at `index` (counted from 1) out of the playlist.
pub fn playlist_remove(index: u32) -> String {
    element("PlaylistRemove", &[("index", index.to_string().into())])
}

/// What HQPlayer shows for a Song it was handed by URL. It does not work out
/// the length of an HTTP stream itself, so that has to come from here.
#[derive(Debug, Clone, Copy, Default)]
pub struct Metadata<'a> {
    pub title: Option<&'a str>,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub length_ms: Option<u64>,
}

/// Append the Song at `uri` to the playlist. Added while another plays, it is
/// what HQPlayer moves on to, gaplessly, when that one ends.
///
/// `legacy` is the HQPlayer 4 form, which knows only `uri`, `queued` and
/// `clear`. HQPlayer 5 and later also take `start`, `freewheel` (fetch as fast
/// as possible rather than at playback speed) and the metadata.
pub fn playlist_add(uri: &str, meta: &Metadata<'_>, legacy: bool) -> String {
    // `queued="1"` and `clear="1"` are both known to misbehave (the latter is
    // ignored while playing); every working client clears on its own and
    // sends these.
    let mut xml = format!(r#"{DECLARATION}<PlaylistAdd uri="{}" queued="0" clear="0""#, escape(uri));
    if legacy {
        xml.push_str("/>");
        return xml;
    }
    xml.push_str(r#" start="0" freewheel="1"><metadata"#);
    // HQPlayer's word for the title is `song`.
    let fields = [("song", meta.title), ("artist", meta.artist), ("album", meta.album)];
    for (key, value) in fields {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            xml.push_str(&format!(r#" {key}="{}""#, escape(value)));
        }
    }
    if let Some(ms) = meta.length_ms.filter(|ms| *ms > 0) {
        xml.push_str(&format!(r#" length="{:.3}""#, ms as f64 / 1000.0));
    }
    xml.push_str("/></PlaylistAdd>");
    xml
}

// ---- replies --------------------------------------------------------------

/// The root element of a document: its name, attributes, and the text
/// directly inside it. `None` for anything that is not well-formed so far.
pub(super) struct Root {
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub text: String,
}

pub(super) fn root_element(xml: &str) -> Option<Root> {
    let mut reader = Reader::from_str(xml);
    let mut root: Option<Root> = None;
    let mut depth = 0usize;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) | Event::Empty(e) if root.is_none() => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let mut attributes = HashMap::new();
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let value = attr.normalized_value(XmlVersion::Implicit1_0).ok()?.into_owned();
                    attributes.insert(key, value);
                }
                let empty = xml[..reader.buffer_position() as usize].trim_end().ends_with("/>");
                root = Some(Root { name, attributes, text: String::new() });
                if empty {
                    return root;
                }
                depth = 1;
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return root;
                }
            }
            Event::Text(t) if depth == 1 => {
                if let (Some(root), Ok(text)) = (root.as_mut(), t.decode()) {
                    root.text.push_str(&text);
                }
            }
            Event::GeneralRef(r) if depth == 1 => {
                // `&amp;` and friends inside the text.
                if let (Some(root), Ok(Some(c))) = (root.as_mut(), r.resolve_char_ref()) {
                    root.text.push(c);
                } else if let (Some(root), Ok(name)) = (root.as_mut(), r.decode()) {
                    let c = match name.as_ref() {
                        "lt" => '<',
                        "gt" => '>',
                        "amp" => '&',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => continue,
                    };
                    root.text.push(c);
                }
            }
            Event::Eof => return root.filter(|_| depth == 0),
            _ => {}
        }
    }
}

/// The attributes of the first `name` element anywhere in `xml`, such as the
/// `<metadata/>` inside a status.
fn element_attributes(xml: &str, name: &str) -> Option<HashMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == name.as_bytes() => {
                let mut attributes = HashMap::new();
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let value = attr.normalized_value(XmlVersion::Implicit1_0).ok()?.into_owned();
                    attributes.insert(key, value);
                }
                return Some(attributes);
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// The command a request or reply is about: its root element's name. Replies
/// are named after the request they answer.
pub fn command_name(xml: &str) -> Option<String> {
    root_element(xml).map(|root| root.name)
}

/// Whether `xml` holds one whole document yet: its root element closed.
pub fn is_complete(xml: &str) -> bool {
    let mut reader = Reader::from_str(xml);
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return true;
                }
            }
            Ok(Event::Empty(_)) if depth == 0 => return true,
            Ok(Event::Eof) | Err(_) => return false,
            Ok(_) => {}
        }
    }
}

/// The reason a reply gives for refusing its request, if it is a refusal:
/// `result="Error"`, with the reason as the element's text.
pub fn failure(xml: &str) -> Option<String> {
    let root = root_element(xml)?;
    let refused = root.attributes.get("result").is_some_and(|r| r.eq_ignore_ascii_case("error"));
    refused.then(|| {
        let reason = root.text.trim();
        if reason.is_empty() {
            format!("{} failed", root.name)
        } else {
            reason.to_owned()
        }
    })
}

fn number<T: std::str::FromStr>(attributes: &HashMap<String, String>, key: &str) -> Option<T> {
    attributes.get(key).and_then(|v| v.trim().parse().ok())
}

fn text(attributes: &HashMap<String, String>, key: &str) -> Option<String> {
    attributes.get(key).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// What an HQPlayer says it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// The product: "Signalyst HQPlayer Desktop".
    pub name: String,
    /// The major version only: "5".
    pub version: Option<String>,
    /// The name of the computer it runs on.
    pub computer: Option<String>,
}

impl Info {
    /// The major version, when it reads as one.
    pub fn major_version(&self) -> Option<u32> {
        self.version.as_deref()?.split('.').next()?.trim().parse().ok()
    }
}

pub fn parse_info(xml: &str) -> Option<Info> {
    let root = root_element(xml)?;
    if root.name != "GetInfo" {
        return None;
    }
    Some(Info {
        name: text(&root.attributes, "product").unwrap_or_else(|| "HQPlayer".into()),
        version: text(&root.attributes, "version"),
        computer: text(&root.attributes, "name"),
    })
}

/// The volume HQPlayer allows, in dB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeRange {
    pub min_db: f32,
    pub max_db: f32,
    /// HQPlayer's volume can be turned at all. Set to fixed in HQPlayer, it
    /// refuses every change, and Reverb's volume control does nothing here.
    pub adjustable: bool,
}

pub fn parse_volume_range(xml: &str) -> Option<VolumeRange> {
    let root = root_element(xml)?;
    if root.name != "VolumeRange" {
        return None;
    }
    let min_db: f32 = number(&root.attributes, "min")?;
    let max_db: f32 = number(&root.attributes, "max")?;
    let enabled = root.attributes.get("enabled").is_none_or(|v| v != "0");
    Some(VolumeRange { min_db, max_db, adjustable: enabled && max_db > min_db })
}

/// Where HQPlayer's transport is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Stopped,
    Paused,
    Playing,
    /// Told to stop and on its way there: `Stop` is answered before the
    /// engine has actually stopped.
    Stopping,
}

/// HQPlayer's answer to how it is doing.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: TransportState,
    /// The playlist entry playing, counted from 1; `None` for none.
    pub track: Option<u32>,
    /// Where the listener is in the Song: HQPlayer's position less the
    /// delay of its output buffer, which at DSD rates runs to seconds.
    pub position_ms: Option<u64>,
    pub length_ms: Option<u64>,
    pub volume_db: Option<f32>,
    /// "PCM", "SDM" for DSD output, or HQPlayer's own word for anything else.
    pub mode: Option<String>,
    pub filter: Option<String>,
    pub shaper: Option<String>,
    /// The rate HQPlayer sends to the DAC, Hz (the bit rate, for DSD).
    pub output_rate: Option<u32>,
    /// The rate of the Song itself, Hz, as HQPlayer found it in the file.
    pub source_rate: Option<u32>,
}

/// Seconds, whole or not, as milliseconds.
fn millis(attributes: &HashMap<String, String>, key: &str) -> Option<u64> {
    let secs: f64 = number(attributes, key)?;
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as u64)
}

pub fn parse_status(xml: &str) -> Option<Status> {
    let root = root_element(xml)?;
    if root.name != "Status" {
        return None;
    }
    let a = &root.attributes;
    let state = match number::<u32>(a, "state")? {
        1 => TransportState::Paused,
        2 => TransportState::Playing,
        3 => TransportState::Stopping,
        _ => TransportState::Stopped,
    };
    let output_delay_ms = number::<u64>(a, "output_delay").unwrap_or(0) / 1000;
    // Only there while a Song is loaded.
    let metadata = element_attributes(xml, "metadata").unwrap_or_default();
    Some(Status {
        state,
        track: number(a, "track").filter(|track: &u32| *track > 0),
        position_ms: millis(a, "position").map(|ms| ms.saturating_sub(output_delay_ms)),
        length_ms: millis(a, "length").filter(|ms| *ms > 0),
        volume_db: number(a, "volume"),
        mode: text(a, "active_mode").map(|mode| mode_name(&mode)),
        filter: text(a, "active_filter"),
        shaper: text(a, "active_shaper"),
        output_rate: number(a, "active_rate").filter(|rate: &u32| *rate > 0),
        source_rate: number(&metadata, "samplerate").filter(|rate: &u32| *rate > 0),
    })
}

/// "SDM (DSD)" and the like read as plain "SDM", so a DSD output can be told
/// from a PCM one.
fn mode_name(mode: &str) -> String {
    if mode.to_ascii_uppercase().starts_with("SDM") {
        "SDM".into()
    } else {
        mode.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_single_elements_with_escaped_attributes() {
        assert_eq!(play(), r#"<?xml version="1.0" encoding="UTF-8"?><Play last="0"/>"#);
        assert_eq!(seek(42), r#"<?xml version="1.0" encoding="UTF-8"?><Seek position="42"/>"#);
        assert_eq!(
            set_volume(-12.34),
            r#"<?xml version="1.0" encoding="UTF-8"?><Volume value="-12.3"/>"#
        );
        assert_eq!(
            status(),
            r#"<?xml version="1.0" encoding="UTF-8"?><Status subscribe="0"/>"#,
            "never a bare <Status/>, which subscribes"
        );
        assert_eq!(command_name(&playlist_remove(2)).as_deref(), Some("PlaylistRemove"));
    }

    #[test]
    fn a_playlist_entry_carries_its_url_and_what_to_show() {
        let meta = Metadata {
            title: Some("Rock & \"Roll\""),
            artist: Some("Band"),
            album: None,
            length_ms: Some(200_500),
        };
        let xml = playlist_add("http://10.0.0.2:5000/songs/ab.flac?a=1&b=2", &meta, false);
        assert_eq!(
            xml,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <PlaylistAdd uri=\"http://10.0.0.2:5000/songs/ab.flac?a=1&amp;b=2\" queued=\"0\" clear=\"0\" start=\"0\" freewheel=\"1\">\
             <metadata song=\"Rock &amp; &quot;Roll&quot;\" artist=\"Band\" length=\"200.500\"/>\
             </PlaylistAdd>"
        );
        // And it reads back as what was put in.
        let root = root_element(&xml).unwrap();
        assert_eq!(root.name, "PlaylistAdd");
        assert_eq!(root.attributes["uri"], "http://10.0.0.2:5000/songs/ab.flac?a=1&b=2");
        assert!(is_complete(&xml));

        // HQPlayer 4 gets only what it knows.
        assert_eq!(
            playlist_add("http://x/a.flac", &meta, true),
            r#"<?xml version="1.0" encoding="UTF-8"?><PlaylistAdd uri="http://x/a.flac" queued="0" clear="0"/>"#
        );
    }

    #[test]
    fn documents_are_complete_once_the_root_closes() {
        assert!(is_complete(r#"<?xml version="1.0" encoding="utf-8"?><Play result="OK"/>"#));
        assert!(is_complete("<Status state=\"2\">\n<metadata song=\"x\"/>\n</Status>\n"));
        assert!(!is_complete("<Status state=\"2\">\n<metadata song=\"x\"/>\n"));
        assert!(!is_complete(r#"<?xml version="1.0"?>"#));
        assert!(!is_complete(""));
    }

    #[test]
    fn refusals_carry_their_reason_as_text() {
        assert_eq!(failure(r#"<Play result="OK"/>"#), None);
        assert_eq!(failure(r#"<Status state="0"/>"#), None);
        // Some commands answer with no result at all.
        assert_eq!(failure(r#"<SetAdaptiveVolume/>"#), None);
        assert_eq!(
            failure(r#"<?xml version="1.0" encoding="utf-8"?><Volume result="Error">clPlaylist::GetAlbumGain(): trackn > last</Volume>"#),
            Some("clPlaylist::GetAlbumGain(): trackn > last".into())
        );
        assert_eq!(
            failure(r#"<Seek result="Error">not seekable &amp; such</Seek>"#),
            Some("not seekable & such".into())
        );
        assert_eq!(failure(r#"<Seek result="Error"/>"#), Some("Seek failed".into()));
    }

    #[test]
    fn reads_what_hqplayer_is() {
        // Recorded from HQPlayer Desktop 5.
        let info = parse_info(
            r#"<?xml version="1.0" encoding="utf-8"?><GetInfo engine="5.35.10" name="studio" platform="Linux" product="Signalyst HQPlayer Desktop" version="5"/>"#,
        )
        .unwrap();
        assert_eq!(
            info,
            Info {
                name: "Signalyst HQPlayer Desktop".into(),
                version: Some("5".into()),
                computer: Some("studio".into()),
            }
        );
        assert_eq!(info.major_version(), Some(5));
        assert_eq!(parse_info(r#"<Status state="0"/>"#), None);
    }

    #[test]
    fn reads_the_volume_range() {
        // As HQPlayer Desktop 5 writes them.
        let range = parse_volume_range(
            r#"<?xml version="1.0" encoding="utf-8"?><VolumeRange adaptive="0" enabled="1" max="0.00000000000000000" min="-60.00000000000000000"/>"#,
        );
        assert_eq!(range, Some(VolumeRange { min_db: -60.0, max_db: 0.0, adjustable: true }));
        let fixed = parse_volume_range(r#"<VolumeRange min="-60" max="-3" enabled="0"/>"#).unwrap();
        assert!(!fixed.adjustable);
    }

    #[test]
    fn reads_a_status() {
        let status = parse_status(
            r#"<Status active_bits="1" active_mode="SDM (DSD)" active_filter="poly-sinc-gauss-xla" active_shaper="ASDM7ECv3" active_rate="22579200" length="153.40842708333332" output_delay="1500000" position="61.5" state="2" track="2" tracks_total="2" volume="-39.00000000000000000">
  <metadata bits="24" channels="2" mime="audio/x-flac" samplerate="96000" song="HTTP stream" uri="http://host/x.flac"/>
</Status>"#,
        )
        .unwrap();
        assert_eq!(
            status,
            Status {
                state: TransportState::Playing,
                track: Some(2),
                // 61.5 s in, less 1.5 s still in the output buffer.
                position_ms: Some(60_000),
                length_ms: Some(153_408),
                volume_db: Some(-39.0),
                mode: Some("SDM".into()),
                filter: Some("poly-sinc-gauss-xla".into()),
                shaper: Some("ASDM7ECv3".into()),
                output_rate: Some(22_579_200),
                source_rate: Some(96_000),
            }
        );
    }

    #[test]
    fn reads_an_idle_status() {
        // Recorded from HQPlayer Desktop 5, stopped.
        let idle = parse_status(
            r#"<?xml version="1.0" encoding="utf-8"?><Status active_bits="32" active_channels="2" active_filter="poly-sinc-gauss-long" active_mode="PCM" active_rate="384000" active_shaper="NS5" apod="0" length="0.00000000000000000" output_delay="0" position="0.00000000000000000" state="0" track="0" track_serial="22" tracks_total="0" transport_serial="44" volume="-54.00000000000000000"/>"#,
        )
        .unwrap();
        assert_eq!(idle.state, TransportState::Stopped);
        assert_eq!(idle.track, None, "track 0 is no track");
        assert_eq!(idle.length_ms, None);
        assert_eq!(idle.mode.as_deref(), Some("PCM"));
        assert_eq!(idle.volume_db, Some(-54.0));
        assert_eq!(idle.source_rate, None, "no Song, no metadata");

        let stopping = parse_status(r#"<Status state="3"/>"#).unwrap();
        assert_eq!(stopping.state, TransportState::Stopping);
        assert_eq!(parse_status(r#"<Status/>"#), None, "no state, no status");
    }
}
