//! Asking the sound server to let go of a card (Linux).
//!
//! A raw ALSA device can only be opened by one process, and on any modern
//! desktop that process is PipeWire (or PulseAudio). Simply taking the card
//! while the server still has a node on it is not enough: the server keeps
//! trying to open the device it believes it owns, fails with `EBUSY`, suspends,
//! and retries every few seconds. Each of those attempts republishes the sink's
//! volume, which the desktop reads as "the user changed the volume" and answers
//! with its volume OSD — every five seconds, for as long as playback lasts.
//!
//! So the server has to be told. Both servers implement the same D-Bus
//! handshake for exactly this, `org.freedesktop.ReserveDevice1`: whoever owns
//! the well-known name for a card owns the card. The server reserves every card
//! it manages, yields to anyone who wants it more, and tears its node down
//! while we hold the name — no node, no retries, no OSD. Releasing the name
//! hands the card back and the sink returns, default-sink preference and all.
//!
//! Best-effort throughout: if the server does not implement the handshake we
//! simply take the card the way we did before.

use std::path::Path;
use std::time::{Duration, Instant};

/// How long to wait for a card to actually change hands. The server closes and
/// opens the device asynchronously, and so does cpal.
const HANDOVER_TIMEOUT: Duration = Duration::from_millis(1500);

/// What we tell the server our claim is worth. It reserves the cards it manages
/// at -20 and releases them to any higher bidder, so anything above that wins.
const PRIORITY: i32 = 10;

fn object_path(card: u32) -> String {
    format!("/org/freedesktop/ReserveDevice1/Audio{card}")
}

fn service_name(card: u32) -> String {
    format!("org.freedesktop.ReserveDevice1.Audio{card}")
}

/// The ALSA card a `hw:`/`plughw:`/`sysdefault:` id addresses. The card is
/// either an index ("1") or an id ("BTR17"); both appear in the wild.
fn card_of(alsa_id: &str) -> Option<&str> {
    let pcm = alsa_id.strip_prefix("alsa:")?;
    let rest = pcm
        .strip_prefix("hw:")
        .or_else(|| pcm.strip_prefix("plughw:"))
        .or_else(|| pcm.strip_prefix("sysdefault:"))?;
    rest.split(',').find_map(|part| part.strip_prefix("CARD="))
}

/// The card number a device id refers to. Reservations are per card number, so
/// an id that names its card ("BTR17") has to be resolved; `/proc/asound` links
/// the two.
pub fn card_number(alsa_id: &str) -> Option<u32> {
    let card = card_of(alsa_id)?;
    if let Ok(number) = card.parse() {
        return Some(number);
    }
    let target = std::fs::read_link(Path::new("/proc/asound").join(card)).ok()?;
    target.to_str()?.strip_prefix("card")?.parse().ok()
}

/// True while no process holds a playback PCM on this card.
fn card_is_free(card: u32) -> bool {
    let dir = format!("/proc/asound/card{card}");
    let Ok(entries) = std::fs::read_dir(&dir) else { return true };
    for pcm in entries.flatten() {
        let name = pcm.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("pcm") && name.ends_with('p')) {
            continue;
        }
        for sub in std::fs::read_dir(pcm.path()).into_iter().flatten().flatten() {
            match std::fs::read_to_string(sub.path().join("hw_params")) {
                Ok(text) if !text.trim_start().starts_with("closed") => return false,
                _ => {}
            }
        }
    }
    true
}

fn wait_until(card: u32, free: bool) -> bool {
    let deadline = Instant::now() + HANDOVER_TIMEOUT;
    while card_is_free(card) != free && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    card_is_free(card) == free
}

/// Our end of the handshake. Owning the name is what claims the card; this
/// exists so whoever wants the card next can ask us for it, as we asked the
/// server.
struct Reservation {
    device: String,
}

#[zbus::interface(name = "org.freedesktop.ReserveDevice1")]
impl Reservation {
    /// Someone else wants the card. We are playing to it, so no — they can
    /// have it when playback moves elsewhere or Reverb exits.
    fn request_release(&self, _priority: i32) -> bool {
        log::info!("declined a request to release {}", self.device);
        false
    }

    #[zbus(property)]
    fn priority(&self) -> i32 {
        PRIORITY
    }

    #[zbus(property)]
    fn application_name(&self) -> String {
        "Reverb".to_owned()
    }

    #[zbus(property)]
    fn application_device_name(&self) -> String {
        self.device.clone()
    }
}

/// A card claimed from the sound server, held for as long as the output stream
/// is open and handed back when this is dropped.
///
/// Nothing outlives the process: a D-Bus name belongs to a connection, so a
/// crash — or a `SIGKILL`, which runs no destructors — releases the claim as
/// surely as a clean exit does.
pub struct CardClaim {
    /// Owning the name *is* the claim; dropping the connection ends it.
    connection: zbus::blocking::Connection,
    card: u32,
}

impl CardClaim {
    /// Take `alsa_id`'s card from the sound server. `None` when there is no
    /// card to take or the server does not answer the handshake — in which case
    /// opening the device may still work, just less politely.
    pub fn acquire(alsa_id: &str) -> Option<Self> {
        let card = card_number(alsa_id)?;
        match Self::request(card) {
            Ok(claim) => {
                if wait_until(card, true) {
                    log::info!("sound server released card {card}");
                } else {
                    log::warn!("card {card} is still in use after claiming it");
                }
                Some(claim)
            }
            Err(e) => {
                log::info!("could not claim card {card} from the sound server: {e}");
                None
            }
        }
    }

    fn request(card: u32) -> Result<Self, zbus::Error> {
        use zbus::fdo::{RequestNameFlags, RequestNameReply};

        let name = service_name(card);
        let path = object_path(card);
        let connection = zbus::blocking::Connection::session()?;
        connection
            .object_server()
            .at(path.as_str(), Reservation { device: format!("Audio{card}") })?;

        let dbus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
        let name = zbus::names::WellKnownName::try_from(name.as_str())?;
        let mut reply = dbus.request_name(name.clone(), RequestNameFlags::DoNotQueue.into())?;

        // Nobody was reserving the card, which means no sound server is playing
        // this game — so nothing has been agreed and nothing will stay off the
        // card. See the "Direct output on Linux" section of the README.
        if reply == RequestNameReply::PrimaryOwner {
            log::warn!(
                "card {card} was not reserved by the sound server: device reservation looks \
                 disabled, so the server may keep competing for the card"
            );
        }

        // The server holds the name at all times, so the usual answer is "taken"
        // and the claim is made by asking its owner to stand down first.
        if reply != RequestNameReply::PrimaryOwner {
            let owner = zbus::blocking::Proxy::new(
                &connection,
                name.clone(),
                path.as_str(),
                "org.freedesktop.ReserveDevice1",
            )?;
            if !owner.call::<_, _, bool>("RequestRelease", &(PRIORITY,))? {
                return Err(zbus::Error::Failure(format!(
                    "the owner of {name} will not give up card {card}"
                )));
            }
            reply = dbus.request_name(
                name.clone(),
                RequestNameFlags::DoNotQueue | RequestNameFlags::ReplaceExisting,
            )?;
        }

        if reply != RequestNameReply::PrimaryOwner {
            return Err(zbus::Error::Failure(format!("could not take {name}: {reply:?}")));
        }
        Ok(Self { connection, card })
    }

    pub fn card(&self) -> u32 {
        self.card
    }
}

impl Drop for CardClaim {
    fn drop(&mut self) {
        // Wait for our own stream to have let go before the server is allowed
        // back in: cpal closes the ALSA device from its own thread, and a
        // server that fails to re-open the card does not try again — it drops
        // the device, leaving the user with no sink for it at all.
        if !wait_until(self.card, true) {
            log::warn!("handing card {} back while it is still open", self.card);
        }
        let name = service_name(self.card);
        let released = zbus::names::WellKnownName::try_from(name.as_str())
            .ok()
            .and_then(|name| {
                let dbus = zbus::blocking::fdo::DBusProxy::new(&self.connection).ok()?;
                dbus.release_name(name).ok()
            });
        match released {
            Some(_) => log::info!("card {} handed back to the sound server", self.card),
            None => log::warn!("could not hand card {} back", self.card),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::card_of;

    #[test]
    fn reads_the_card_from_direct_ids() {
        assert_eq!(card_of("alsa:hw:CARD=1,DEV=0"), Some("1"));
        assert_eq!(card_of("alsa:plughw:CARD=BTR17,DEV=0"), Some("BTR17"));
        assert_eq!(card_of("alsa:hw:CARD=1"), Some("1"));
        // The card's own default PCM opens the hardware just as directly.
        assert_eq!(card_of("alsa:sysdefault:CARD=BTR17"), Some("BTR17"));
    }

    #[test]
    fn ignores_ids_that_are_not_a_specific_card() {
        // Sound-server routes and other hosts have no card to claim.
        assert_eq!(card_of("alsa:default"), None);
        assert_eq!(card_of("alsa:pipewire"), None);
        assert_eq!(card_of("alsa:sysdefault"), None);
        assert_eq!(card_of("wasapi:{0.0.0.000}"), None);
    }
}
