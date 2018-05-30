use hyper::Uri as HyperUri;

#[derive(Debug)]
#[derive(Clone)]
pub struct AppConfig {
    // This contains the
    pub upstream: HyperUri,
    pub proxy: Option<String>,
    pub bind_target: HyperUri,
    pub cache_folder: String,
    pub cache_folder_size: u64,
}

impl AppConfig {
    pub fn upstream(&self) -> HyperUri {
        self.upstream.clone()
    }
}
