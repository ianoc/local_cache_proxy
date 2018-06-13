use hyper::Uri as HyperUri;

#[derive(Debug, Clone)]
pub struct AppConfig {
    // This contains the upstream we fetch from
    pub primary_upstream: HyperUri,
    // we will try put to these upstreams too
    pub secondary_upstreams: Vec<HyperUri>,
    pub proxy: Option<String>,
    pub bind_target: HyperUri,
    pub cache_folder: String,
    pub cache_folder_size: u64,
    pub maximum_download_size: u64,
    pub maximum_upload_size: u64,
}

impl AppConfig {
    pub fn primary_upstream(&self) -> HyperUri {
        self.primary_upstream.clone()
    }

    pub fn secondary_upstreams(&self) -> Vec<HyperUri> {
        self.secondary_upstreams.clone()
    }

    pub fn proxy(&self) -> Option<String> {
        self.proxy.clone()
    }
}
