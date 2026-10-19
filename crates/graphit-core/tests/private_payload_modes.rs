use graphit_core::artifact_store::PayloadMode;
use std::{fs, io::Write};

#[test]
fn desired_private_modes_match_exclusive_native_outputs() {
    let file_mode = PayloadMode::private_file();
    let directory_mode = PayloadMode::private_directory();
    let temporary = tempfile::tempdir().unwrap();
    assert!(fs::read_dir(temporary.path()).unwrap().next().is_none());

    let directory_path = temporary.path().join("private-output");
    let directory = graphit_fs::create_private_directory(&directory_path).unwrap();
    graphit_fs::verify_private_directory(&directory).unwrap();
    assert!(
        PayloadMode::from_permissions(&directory.metadata().unwrap().permissions()).unwrap()
            == directory_mode
    );

    let file_path = directory_path.join("payload");
    let mut file = graphit_fs::create_private_file(&file_path).unwrap();
    file.write_all(b"private output remains owned by its native producer")
        .unwrap();
    graphit_fs::verify_private_file(&file).unwrap();
    assert!(
        PayloadMode::from_permissions(&file.metadata().unwrap().permissions()).unwrap()
            == file_mode
    );
    assert_eq!(
        fs::read(&file_path).unwrap(),
        b"private output remains owned by its native producer"
    );
}
