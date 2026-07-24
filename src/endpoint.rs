#[derive(Debug, Clone)]
pub struct Endpoint {
    pub index: usize,
    pub guid: String,
    pub name: String,
    pub kind: EndpointKind,
    pub sfx: Option<String>,
    pub mfx: Option<String>,
    pub efx: Option<String>,
    pub sfx_is_system: bool,
    pub mfx_is_system: bool,
    pub efx_is_system: bool,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bit_depth: Option<u16>,
    pub channel_mask: Option<u32>,
    pub endpoint_flags: Option<u32>,
    pub disable_enhancements: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EndpointKind {
    Playback,
    Capture,
}

impl std::fmt::Display for EndpointKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointKind::Playback => write!(f, "Playback"),
            EndpointKind::Capture => write!(f, "Capture"),
        }
    }
}

impl EndpointKind {
    pub fn reg_path(&self) -> &'static str {
        match self {
            EndpointKind::Playback => "Render",
            EndpointKind::Capture => "Capture",
        }
    }
}