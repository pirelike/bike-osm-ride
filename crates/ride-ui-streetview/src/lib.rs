//! Google Street View for the ride screen, with the rider's **own** API key.
//!
//! Google only offers Street View through its APIs, so the user pastes a key from their own
//! Google Cloud project (Maps Embed API + Street View Static API enabled) into the settings.
//! This crate then:
//!
//! 1. asks the Street View Static API **metadata** endpoint (JSON only, free of charge) for
//!    the panorama nearest to a route point ([`client`]),
//! 2. shows a **downloaded image** of that panorama when the route was saved for offline use
//!    ([`imagestore`]), otherwise that panorama in Google's own Maps Embed API iframe
//!    ([`url`], [`panel`]),
//! 3. decides when a new one is needed and where the camera looks ([`planner`]),
//! 4. optionally remembers the panorama **ids** per route on disk and resolves them ahead of
//!    the rider ([`cache`], [`prefetch`]), and — with the opt-in offline option — downloads
//!    one image per image bucket of the route ([`download`], [`image`]).
//!
//! Downloading imagery only happens on an explicit user action ("Letöltés"), one image per
//! 75–250 m of route (at most [`imagestore::MAX_IMAGES`]), and the files are shown for at
//! most [`imagestore::IMAGE_TTL_S`] (30 days). After such a download the same route rides
//! with no Google request and no API key at all. Only that downloaded image, the panorama
//! ids and our own route coordinates are stored; the key is never logged, never part of an
//! error, and only sent to Google (or, for tests and QA, to a loopback mock chosen with
//! `RIDE_STREETVIEW_BASE`, see [`url::Endpoints`] and [`mock`]).
//!
//! UI: [`StreetViewPanel`] (stateful, used by the shell), [`StreetViewView`] (presentational,
//! used by the SSR preview) and the stylesheet [`CSS`], class names prefixed `sv`.

pub mod cache;
pub mod client;
pub mod download;
pub mod error;
pub mod image;
pub mod imagestore;
pub mod mock;
pub mod panel;
pub mod planner;
pub mod prefetch;
pub mod url;

pub use cache::{PanoCache, BUCKET_M};
pub use client::{MetadataClient, PanoInfo};
pub use download::{DownloadHandle, Downloader, Progress as DownloadProgress};
pub use error::SvError;
pub use image::ImageFetcher;
pub use imagestore::{ImageOverview, ImageStore};
pub use panel::{PanelStatus, PanelView, StreetViewPanel, StreetViewView, CSS};
pub use planner::{Availability, Effect, Outcome, Plan, Planner, PlannerConfig};
pub use prefetch::{PrefetchConfig, PrefetchHandle, Prefetcher, Progress};
pub use url::{ApiKey, EmbedTarget, EmbedUrl, Endpoints, View};
