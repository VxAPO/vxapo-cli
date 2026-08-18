use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

static LANG: OnceLock<Lang> = OnceLock::new();

pub fn set_lang(lang: Lang) {
    let _ = LANG.set(lang);
}

pub fn lang() -> Lang {
    *LANG.get().unwrap_or(&Lang::Zh)
}

pub fn tr<'a>(zh: &'a str, en: &'a str) -> &'a str {
    if lang() == Lang::En {
        en
    } else {
        zh
    }
}
