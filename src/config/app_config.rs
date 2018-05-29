use hyper::Uri as HyperUri;

#[derive(Debug)]
pub struct AppConfig {
    // This contains the
    pub proxy: Option<String>,
    pub bind_target: HyperUri,
    pub cache_folder: String
}

