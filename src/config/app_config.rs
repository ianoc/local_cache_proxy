use hyper::Uri as HyperUri;
use std;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub prefix: String,
}

impl S3Config {
    pub fn bucket(&self) -> String {
        self.bucket.clone()
    }
    pub fn prefix(&self) -> String {
        self.prefix.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    // This contains the upstream we fetch from
    pub upstream: HyperUri,
    pub proxy: Option<String>,
    pub bind_target: HyperUri,
    pub cache_folder: String,
    pub cache_folder_size: u64,
    pub maximum_download_size: u64,
    pub maximum_upload_size: u64,
    pub idle_time_terminate: Option<Duration>,
}

impl AppConfig {
    pub fn upstream(&self) -> HyperUri {
        self.upstream.clone()
    }

    pub fn proxy(&self) -> Option<String> {
        self.proxy.clone()
    }

    pub fn str_to_ms(s: &str) -> Result<Duration, std::num::ParseIntError> {
        s.parse::<u64>().map(|e| Duration::from_millis(e))
    }
}
