/// The tests rely on qemu-img and nbdinfo to be installed.
#[cfg(test)]
mod tests {
    use nbd::{unix, Export};
    use serde_json::{self, Value};
    use std::{
        path::Path,
        process::Command,
        sync::{atomic::AtomicBool, Arc},
        thread::{self, JoinHandle},
    };

    const TEST_FILE: &str = "/tmp/test.img";

    fn create_export_file() -> Result<(), Box<dyn std::error::Error>> {
        Command::new("qemu-img")
            .arg("create")
            .arg("-f")
            .arg("qcow2")
            .arg(TEST_FILE)
            .arg("1g")
            .output()?;

        Ok(())
    }

    fn start_unix_server(
        stop_server: Arc<AtomicBool>,
    ) -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
        create_export_file()?;
        let export = Export::init_export(
            TEST_FILE.to_string(),
            String::from("test"),
            String::from("test"),
        )?;

        let handle = thread::spawn(move || {
            unix::start_unix_socket_server(&export, Path::new("/tmp/nbd.sock"), &stop_server)
                .unwrap();
        });
        Ok(handle)
    }

    #[test]
    pub fn test_qemu_img_info() {
        let stop_server = Arc::new(AtomicBool::new(false));
        let handle = start_unix_server(stop_server.clone()).unwrap();

        let output = Command::new("qemu-img")
            .arg("info")
            .arg("nbd+unix://?socket=/tmp/nbd.sock")
            .arg("--output")
            .arg("json")
            .output()
            .expect("failed to execute process");
        let value = String::from_utf8_lossy(&output.stdout);
        let v: Value = serde_json::from_str(&value).unwrap();

        stop_server.store(true, std::sync::atomic::Ordering::SeqCst);
        handle.join().unwrap();

        assert!(output.status.success());
        assert_eq!(v["format"].as_str().unwrap(), "qcow2");
        assert_eq!(v["virtual-size"].as_u64(), Some(1073741824 as u64));

        // Cleanup
        std::fs::remove_file(TEST_FILE).unwrap();
    }
}
