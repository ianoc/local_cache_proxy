use config::AppConfig;
use protobuf::{CodedInputStream, Message}; //, ProtobufResult, RepeatedField};
use std::io::BufReader;
// use std::io::{self, stdin, BufRead, BufReader};
use action_result::ActionResult;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::thread;

//from https://doc.rust-lang.org/stable/rust-by-example/std_misc/fs.html
fn touch(path: &Path) -> io::Result<()> {
    match OpenOptions::new().create(true).write(true).open(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn process_existing_action_caches(config: AppConfig) {
    thread::spawn(move || {
        let paths = fs::read_dir(&config.cache_folder).unwrap();

        for p in paths {
            for pe in p.iter() {
                process_action_cache_response(&config, &pe.file_name().into_string().unwrap())
                    .unwrap_or(());
            }
        }
    });
}
pub fn process_action_cache_response(
    config: &AppConfig,
    downloaded_file: &String,
) -> Result<(), String> {
    if downloaded_file.starts_with("ac__") {
        let data_source_path = Path::new(&config.cache_folder).join(downloaded_file);
        info!(
            "Processing for action cache entries: {:?}",
            data_source_path
        );

        let mut s = ActionResult::new();

        let file = File::open(data_source_path).map_err(|e| e.to_string())?;
        let mut br = BufReader::new(file);
        let mut cis = CodedInputStream::from_buffered_reader(&mut br);
        s.merge_from(&mut cis).map_err(|e| e.to_string())?;

        for f in s.output_files.iter() {
            match f.digest.as_ref() {
                Some(h) => {
                    let file_name = format!("enable_cas__{}", h.hash);
                    touch(&Path::new(&config.cache_folder).join(&file_name)).map_err(|e| {
                        warn!("{:?}", e);
                        e.to_string()
                    })?;
                }
                None => (),
            }
        }

        match s.stdout_digest.as_ref() {
            Some(h) => {
                let file_name = format!("enable_cas__{}", h.hash);
                touch(&Path::new(&config.cache_folder).join(&file_name))
                    .map_err(|e| e.to_string())?;
            }
            None => (),
        }
        match s.stderr_digest.as_ref() {
            Some(h) => {
                let file_name = format!("enable_cas__{}", h.hash);
                touch(&Path::new(&config.cache_folder).join(&file_name))
                    .map_err(|e| e.to_string())?;
            }
            None => (),
        }
    }

    Ok(())
}
