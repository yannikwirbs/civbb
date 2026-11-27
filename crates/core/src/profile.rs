//! Tracy zones behind the off-by-default `profile` feature (ADR 0103): without it zones are empty values and no Tracy
//! code is built.

/// An open zone; it closes when dropped.
#[must_use = "a zone closes when dropped"]
pub struct Zone {
    #[cfg(feature = "profile")]
    _span: Option<tracy_client::Span>,
}

/// Starts the Tracy client, which then waits for a server to connect. Zones opened before this record nothing.
pub fn start() {
    #[cfg(feature = "profile")]
    tracy_client::Client::start();
}

/// Opens a zone named `name` on the calling thread, at the caller's source location.
#[track_caller]
#[inline]
pub fn zone(name: &str) -> Zone {
    #[cfg(feature = "profile")]
    {
        let at = std::panic::Location::caller();
        Zone {
            _span: tracy_client::Client::running().map(|c| c.span_alloc(Some(name), "", at.file(), at.line(), 0)),
        }
    }
    #[cfg(not(feature = "profile"))]
    {
        let _ = name;
        Zone {}
    }
}

/// Marks the end of a simulation frame.
#[inline]
pub fn frame_mark() {
    #[cfg(feature = "profile")]
    if let Some(c) = tracy_client::Client::running() {
        c.frame_mark();
    }
}
