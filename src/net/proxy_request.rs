use hyper::Uri as HyperUri;
use net::server_error::ServerError;

pub struct ProxyRequest {
    pub repo: String,
    pub tpe: String, // ac or cas
    pub digest: String,
}
impl ProxyRequest {
    pub fn file_name(self: &Self) -> String {
        format!("{}__{}", self.tpe, self.digest).to_string()
    }
    pub fn build_query_uri(self: &Self, upstream_uri: &HyperUri) -> Result<HyperUri, ServerError> {
        let upstream_str = format!("{}", upstream_uri)
            .trim_right_matches('/')
            .to_string();
        format!(
            "{}/{}/repo={}/{}",
            upstream_str, self.tpe, self.repo, self.digest
        ).parse()
            .map_err(From::from)
    }

    pub fn new(uri: &HyperUri) -> ProxyRequest {
        let path: &str = uri.path().trim_matches('/');
        let elements: Vec<&str> = path.split('/').collect();
        if path.starts_with("repo=") {
            let repo_name: &str = {
                let parts: Vec<&str> = elements[0].split('=').collect();
                parts[1]
            };
            ProxyRequest {
                repo: repo_name.to_string(),
                tpe: elements[1].to_string(),
                digest: elements[2].to_string(),
            }
        } else if elements.len() == 2 && (elements[0] == "ac" || elements[0] == "cas") {
            ProxyRequest {
                repo: "unknown".to_string(),
                tpe: elements[0].to_string(),
                digest: elements[1].to_string(),
            }
        } else if path == "/" {
            ProxyRequest {
                repo: "unknown".to_string(),
                tpe: "unknown".to_string(),
                digest: "index.html".to_string(),
            }
        } else {
            ProxyRequest {
                repo: "unknown".to_string(),
                tpe: "unknown".to_string(),
                digest: path.replace('/', "__"),
            }
        }
    }
}
