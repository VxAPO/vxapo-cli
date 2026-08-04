use std::collections::HashMap;
use std::sync::LazyLock;

pub struct PropertyMeta {
    pub name: &'static str,
    pub description: &'static str,
}

pub static KNOWN_PROPERTIES: LazyLock<HashMap<&'static str, PropertyMeta>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("{a45c254e-df1c-4efd-8020-67d146a850e0},2",  PropertyMeta { name: "ConnectionName",      description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},6",  PropertyMeta { name: "DeviceName",           description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},2",  PropertyMeta { name: "DeviceDesc",           description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},0",  PropertyMeta { name: "DeviceState",          description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},3",  PropertyMeta { name: "FormFactor",           description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},4",  PropertyMeta { name: "CompositorID",         description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},9",  PropertyMeta { name: "AudioEndpoint_Flags",  description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},3",  PropertyMeta { name: "EP_FormFactor",        description: "" });
    m.insert("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},3",  PropertyMeta { name: "ChannelMask",          description: "" });
    m.insert("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5",  PropertyMeta { name: "DisableEnhancements",  description: "" });
    m.insert("{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0",  PropertyMeta { name: "AudioFormat",          description: "" });
    m.insert("{80f111c3-b103-42e1-afb6-db7a6fa8be1f},0",  PropertyMeta { name: "DeviceHWId",           description: "" });
    m.insert("{a45c254e-df1c-4efd-8020-67d146a850e0},24", PropertyMeta { name: "DeviceInterface",      description: "" });
    m.insert("{9c119480-ddc2-4954-a150-5bd240d454ad},1",  PropertyMeta { name: "DeviceInterfacePath",  description: "" });
    m.insert("{9c119480-ddc2-4954-a150-5bd240d454ad},2",  PropertyMeta { name: "DeviceInstanceId",     description: "" });
    m.insert("{233164c8-1b2c-4c7d-bc68-b671687a2567},1",  PropertyMeta { name: "DeviceInterface",      description: "" });
    m.insert("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1",  PropertyMeta { name: "LFX_APO",              description: "" });
    m.insert("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2",  PropertyMeta { name: "GFX_APO",              description: "" });
    m.insert("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5",  PropertyMeta { name: "SFX_APO",              description: "" });
    m.insert("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6",  PropertyMeta { name: "MFX_APO",              description: "" });
    m.insert("{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7",  PropertyMeta { name: "EFX_APO",              description: "" });
    m.insert("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},2",  PropertyMeta { name: "EndpointGUID",         description: "" });
    m.insert("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},7",  PropertyMeta { name: "FullRangeSpeakers",    description: "" });
    m.insert("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},8",  PropertyMeta { name: "DeviceClassGuid",      description: "" });
    m.insert("{b3f8fa53-0004-438e-9003-51a46e139bfc},32", PropertyMeta { name: "EngineDeviceFormat",   description: "" });
    m
});

/// APO CLSID → 友好名映射（CLI 引用规范 4.5：EAPO/VxAPO pre/postmix 实读源码确认）。
pub static KNOWN_APO_CLSIDS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("{eacd2258-fcac-4ff4-b36d-419e924a6d79}", "Equalizer APO PreMix");
    m.insert("{ec1cc9ce-faed-4822-828a-82a81a6f018f}", "Equalizer APO PostMix");
    m.insert("{41c34613-d391-459d-a039-72b2b15a1a1d}", "VxAPO PreMix");
    m.insert("{b4a97313-abc0-45ed-9c33-428b20d39428}", "VxAPO PostMix");
    m
});

pub const SYSTEM_APO_CLSIDS: &[&str] = &[
    "{da2c9ece-7418-4906-b4fa-0a00b3eb88aa}",
    "{c9453e73-8c5c-4463-9984-af8bab2f5447}",
    "{7ab03736-d528-4e73-905a-7e5e7f3b0b5c}",
    "{a29eb043-6ce2-4ee2-b38c-f58719e0d88f}",
    "{ab3b404a-b18f-4b4f-b91f-77f2de95eb18}",
    "{5860e1c5-f95c-4a7a-8ec8-8aef24f379a1}",
    "{a296d363-ee83-4af9-9be7-729c1296150a}",
    "{a69c91dc-11c4-414f-a919-4da8ea3f3ca6}",
    "{13ab3ebd-137e-4903-9d89-60be8277fd17}",
    "{6861cfdc-0461-49d5-a8df-be5acd02692f}",
    "{5860e1c5-f95c-4a7a-8ec8-8aef24f379a1}",
    "{00000000-0000-0000-0000-000000000000}",
];

pub const KNOWN_PREFIXES: &[&str] = &[
    "{a45c254e",
    "{b3f8fa53",
    "{d04e05a6",
    "{80f111c3",
    "{9c119480",
    "{1da5d803",
    "{233164c8",
    "{f19f064d",
];

pub fn friendly_name(raw: &str) -> String {
    let lower = raw.to_lowercase();
    KNOWN_PROPERTIES
        .iter()
        .find(|(k, _)| k.to_lowercase() == lower)
        .map(|(_, m)| {
            if m.description.is_empty() {
                m.name.to_string()
            } else {
                format!("{}  {}", m.name, m.description)
            }
        })
        .unwrap_or_else(|| raw.to_string())
}

pub fn is_system_apo(clsid: &str) -> bool {
    let lower = clsid.to_lowercase();
    SYSTEM_APO_CLSIDS.iter().any(|s| *s == lower)
}

pub fn is_known_property(name: &str) -> bool {
    KNOWN_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

pub fn decode_dword_label(name: &str, value: u32) -> String {
    if name == "{b3f8fa53-0004-438e-9003-51a46e139bfc},0" {
        return match value {
            1 => " (ACTIVE)".to_string(),
            2 => " (DISABLED)".to_string(),
            4 => " (NOTPRESENT)".to_string(),
            8 => " (UNPLUGGED)".to_string(),
            _ => format!(" (unknown=0x{value:02x})"),
        };
    }
    if name == "{b3f8fa53-0004-438e-9003-51a46e139bfc},9" {
        return if value == 0 {
            " (enhancements ENABLED)".to_string()
        } else {
            format!(" (flags=0x{value:02x})")
        };
    }
    if name == "{b3f8fa53-0004-438e-9003-51a46e139bfc},3" {
        return match value {
            1 => " (Speakers)".to_string(),
            2 => " (LineLevel)".to_string(),
            3 => " (Headphones)".to_string(),
            4 => " (Microphone)".to_string(),
            5 => " (Headset)".to_string(),
            6 => " (Handset)".to_string(),
            _ => String::new(),
        };
    }
    if name == "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},0" {
        return match value {
            1 => " (Speakers)".to_string(),
            3 => " (Headphones)".to_string(),
            4 => " (Microphone)".to_string(),
            5 => " (Headset)".to_string(),
            _ => String::new(),
        };
    }
    if name == "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},7" {
        return if value == 1 { " (Yes)".to_string() } else { String::new() };
    }
    if name == "{b3f8fa53-0004-438e-9003-51a46e139bfc},32" {
        return if value == 0 { " (default/not set)".to_string() } else { String::new() };
    }
    String::new()
}

pub fn apo_slot_annotation(name: &str) -> &'static str {
    if !name.starts_with("{d04e05a6") {
        return "";
    }
    if let Some(comma) = name.rfind(',') {
        if let Ok(pid) = name[comma + 1..].parse::<u32>() {
            return match pid {
                0 => "  (FX metadata)",
                1 => "  (LFX slot)",
                2 => "  (GFX slot)",
                3 => "  (non-standard)",
                5 => "  (SFX slot)",
                6 => "  (MFX slot)",
                7 => "  (EFX slot)",
                _ => "  (unknown slot)",
            };
        }
    }
    ""
}