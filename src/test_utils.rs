use crate::config::{Latitude, Longitude};

pub struct LatLng {
    pub lat: Latitude,
    pub lng: Longitude,
}

pub const LONDON: LatLng = LatLng {
    lat: 51.51,
    lng: -0.12,
};
