//! Linux Source uses epoll and may retain more descriptors than the login soft limit.
use rustix::process::{Resource, getrlimit, setrlimit};

/// Use the descriptor capacity already admitted by the parent, preserving its hard limit.
pub(crate) fn raise_nofile_soft_limit() -> std::io::Result<()> {
    let mut limit = getrlimit(Resource::Nofile);
    if limit.current != limit.maximum {
        limit.current = limit.maximum;
        setrlimit(Resource::Nofile, limit)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/support/finite_cli.rs"]
mod finite_cli;

#[cfg(test)]
mod tests {
    use super::finite_cli::FiniteOutput;
    use super::*;
    use rustix::process::Rlimit;
    use std::{fs::File, os::fd::AsRawFd, process::Command};

    #[test]
    fn nofile_promotion_preserves_inherited_hard_limit_in_isolated_process() {
        const CHILD: &str = "GRAPHIT_TEST_NOFILE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let parent_limit = getrlimit(Resource::Nofile);
            let module = module_path!().split_once("::").unwrap().1;
            let test = format!(
                "{module}::nofile_promotion_preserves_inherited_hard_limit_in_isolated_process"
            );
            let directory = tempfile::tempdir().unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .env_clear()
                .env("HOME", directory.path())
                .current_dir(directory.path())
                .args(["--exact", &test, "--nocapture"])
                .env(CHILD, "1")
                .finite_output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated NOFILE qualification failed: {output:?}"
            );
            assert!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .contains("NOFILE promotion verified")
            );
            assert_eq!(getrlimit(Resource::Nofile), parent_limit);
            return;
        }

        let inherited = getrlimit(Resource::Nofile);
        let hard = inherited.maximum.unwrap_or(2048).min(2048);
        assert!(
            hard > 1024,
            "native qualification requires a hard limit above 1024"
        );
        setrlimit(
            Resource::Nofile,
            Rlimit {
                current: Some(1024),
                maximum: Some(hard),
            },
        )
        .unwrap();
        raise_nofile_soft_limit().unwrap();
        let promoted = getrlimit(Resource::Nofile);
        assert_eq!(promoted.current, Some(hard));
        assert_eq!(promoted.maximum, Some(hard));
        raise_nofile_soft_limit().unwrap();
        assert_eq!(getrlimit(Resource::Nofile), promoted);

        let mut held = Vec::new();
        loop {
            let file = File::open("/dev/null").unwrap();
            let above_select_limit = file.as_raw_fd() > 1023;
            held.push(file);
            if above_select_limit {
                break;
            }
        }
        println!("NOFILE promotion verified");
    }
}
