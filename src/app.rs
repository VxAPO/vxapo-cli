use crate::endpoint::Endpoint;
use crate::probe;

pub struct App {
    pub endpoints: Vec<Endpoint>,
}

impl App {
    pub fn new() -> Self {
        App { endpoints: Vec::new() }
    }

    pub fn refresh(&mut self) {
        self.endpoints = probe::probe_all();
    }
}
