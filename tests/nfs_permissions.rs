//! NFS Permission Tests for gVisor/Kata/Native runtimes
//!
//! Tests all file operations from the APPS-13266 test matrix.
//! Run as root: cargo test
//! Run as non-root: su nfsb -c "cargo test"
//!
//! Environment variable:
//!   NFS_TEST_PATH - path to NFS mount (default: /mnt/nfs)
//!
//! Expected: ALL tests should PASS if NFS permissions work correctly.
//! If a test fails, that operation doesn't work on the current runtime.

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

/// get test directory path from env or default
fn test_base_path() -> PathBuf {
    std::env::var("NFS_TEST_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/mnt/nfs"))
}

/// create a unique test directory for this test run
fn create_test_dir(name: &str) -> PathBuf {
    let base = test_base_path();
    let uid = unsafe { libc::getuid() };
    let pid = std::process::id();
    let dir_name = format!("test_{}_{}_uid{}", name, pid, uid);
    let path = base.join(dir_name);

    // clean up if exists from previous failed run
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("failed to create test directory");
    path
}

/// cleanup test directory
fn cleanup_test_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

/// get current user info for test output
fn current_user_info() -> String {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    format!("uid={}, gid={}", uid, gid)
}

/// print test header with user context
fn print_test_header(test_name: &str) {
    eprintln!("\n=== {} ===", test_name);
    eprintln!("    User: {}", current_user_info());
}

// ============================================================================
// CREATE OPERATIONS
// ============================================================================

#[test]
fn test_create_file() {
    print_test_header("test_create_file");
    let test_dir = create_test_dir("create_file");
    let file_path = test_dir.join("new_file.txt");

    let result = File::create(&file_path);
    assert!(result.is_ok(), "CREATE FILE FAILED: {:?}", result.err());
    assert!(file_path.exists(), "file should exist after creation");

    let meta = fs::metadata(&file_path).unwrap();
    eprintln!("    File owner: uid={}, gid={}, mode={:o}",
              meta.uid(), meta.gid(), meta.mode() & 0o7777);

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_create_directory() {
    print_test_header("test_create_directory");
    let test_dir = create_test_dir("create_dir");
    let new_dir = test_dir.join("subdir");

    let result = fs::create_dir(&new_dir);
    assert!(result.is_ok(), "CREATE DIRECTORY FAILED: {:?}", result.err());
    assert!(new_dir.is_dir(), "directory should exist after creation");

    let meta = fs::metadata(&new_dir).unwrap();
    eprintln!("    Dir owner: uid={}, gid={}, mode={:o}",
              meta.uid(), meta.gid(), meta.mode() & 0o7777);

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_nested_mkdir_p() {
    print_test_header("test_nested_mkdir_p");
    let test_dir = create_test_dir("nested_mkdir");
    let nested_path = test_dir.join("level1/level2/level3");

    let result = fs::create_dir_all(&nested_path);
    assert!(result.is_ok(), "NESTED MKDIR FAILED: {:?}", result.err());
    assert!(nested_path.is_dir(), "nested directory should exist");

    eprintln!("    Created: {}", nested_path.display());
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// READ OPERATIONS
// ============================================================================

#[test]
fn test_read_file() {
    print_test_header("test_read_file");
    let test_dir = create_test_dir("read_file");
    let file_path = test_dir.join("readable.txt");

    // create file with content
    {
        let mut f = File::create(&file_path).expect("failed to create file");
        f.write_all(b"test content for reading").expect("failed to write");
    }

    // read it back
    let content = fs::read_to_string(&file_path);
    assert!(content.is_ok(), "READ FILE FAILED: {:?}", content.err());
    assert_eq!(content.unwrap(), "test content for reading");

    eprintln!("    Read successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// MODIFY OPERATIONS
// ============================================================================

#[test]
fn test_append_to_file() {
    print_test_header("test_append_to_file");
    let test_dir = create_test_dir("append_file");
    let file_path = test_dir.join("appendable.txt");

    // create file
    {
        let mut f = File::create(&file_path).expect("failed to create file");
        f.write_all(b"initial content\n").expect("failed to write initial");
    }

    // append to file
    let mut file = OpenOptions::new()
        .append(true)
        .open(&file_path);

    assert!(file.is_ok(), "APPEND OPEN FAILED: {:?}", file.err());

    let write_result = file.as_mut().unwrap().write_all(b"appended content\n");
    assert!(write_result.is_ok(), "APPEND WRITE FAILED: {:?}", write_result.err());

    // verify content
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("initial content"), "original content missing");
    assert!(content.contains("appended content"), "appended content missing");

    eprintln!("    Append successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_overwrite_file() {
    print_test_header("test_overwrite_file");
    let test_dir = create_test_dir("overwrite_file");
    let file_path = test_dir.join("overwritable.txt");

    // create file
    {
        let mut f = File::create(&file_path).expect("failed to create file");
        f.write_all(b"original content").expect("failed to write original");
    }

    // overwrite file
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&file_path);

    assert!(file.is_ok(), "OVERWRITE OPEN FAILED: {:?}", file.err());

    let write_result = file.as_mut().unwrap().write_all(b"new content");
    assert!(write_result.is_ok(), "OVERWRITE WRITE FAILED: {:?}", write_result.err());

    // verify content
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "new content");

    eprintln!("    Overwrite successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_write_in_subdirectory() {
    print_test_header("test_write_in_subdirectory");
    let test_dir = create_test_dir("write_subdir");

    // create subdirectory
    let subdir = test_dir.join("subdir");
    fs::create_dir(&subdir).expect("failed to create subdir");

    let file_in_subdir = subdir.join("file.txt");

    // create file in subdirectory
    let mut file = File::create(&file_in_subdir);
    assert!(file.is_ok(), "CREATE IN SUBDIR FAILED: {:?}", file.err());

    let write_result = file.as_mut().unwrap().write_all(b"content in subdir");
    assert!(write_result.is_ok(), "WRITE IN SUBDIR FAILED: {:?}", write_result.err());

    eprintln!("    Write in subdirectory successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_dd_append_seek() {
    print_test_header("test_dd_append_seek");
    let test_dir = create_test_dir("dd_seek");
    let file_path = test_dir.join("seekable.bin");

    // create file with initial data
    {
        let mut f = File::create(&file_path).expect("failed to create file");
        f.write_all(&[0u8; 1024]).expect("failed to write initial data");
    }

    // open, seek, and write (simulates dd with seek)
    let mut file = OpenOptions::new()
        .write(true)
        .open(&file_path);

    assert!(file.is_ok(), "SEEK OPEN FAILED: {:?}", file.err());

    let f = file.as_mut().unwrap();
    let seek_result = f.seek(SeekFrom::Start(512));
    assert!(seek_result.is_ok(), "SEEK FAILED: {:?}", seek_result.err());

    let write_result = f.write_all(&[0xFFu8; 512]);
    assert!(write_result.is_ok(), "SEEK WRITE FAILED: {:?}", write_result.err());

    eprintln!("    Seek+write successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_touch_existing_file() {
    print_test_header("test_touch_existing_file");
    let test_dir = create_test_dir("touch_existing");
    let file_path = test_dir.join("touchable.txt");

    // create file
    File::create(&file_path).expect("failed to create file");

    // touch = open for write (updates mtime)
    let file = OpenOptions::new()
        .write(true)
        .open(&file_path);

    assert!(file.is_ok(), "TOUCH EXISTING FILE FAILED: {:?}", file.err());

    eprintln!("    Touch existing file successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// DELETE OPERATIONS
// ============================================================================

#[test]
fn test_delete_file() {
    print_test_header("test_delete_file");
    let test_dir = create_test_dir("delete_file");
    let file_path = test_dir.join("deletable.txt");

    // create file
    File::create(&file_path).expect("failed to create file");
    assert!(file_path.exists());

    // delete file
    let result = fs::remove_file(&file_path);
    assert!(result.is_ok(), "DELETE FILE FAILED: {:?}", result.err());
    assert!(!file_path.exists(), "file should not exist after deletion");

    eprintln!("    Delete file successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_delete_directory() {
    print_test_header("test_delete_directory");
    let test_dir = create_test_dir("delete_dir");
    let subdir = test_dir.join("deletable_dir");

    fs::create_dir(&subdir).expect("failed to create directory");
    assert!(subdir.is_dir());

    // delete empty directory
    let result = fs::remove_dir(&subdir);
    assert!(result.is_ok(), "DELETE DIRECTORY FAILED: {:?}", result.err());
    assert!(!subdir.exists(), "directory should not exist after deletion");

    eprintln!("    Delete directory successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// RENAME/MOVE OPERATIONS
// ============================================================================

#[test]
fn test_rename_file() {
    print_test_header("test_rename_file");
    let test_dir = create_test_dir("rename_file");
    let src = test_dir.join("original.txt");
    let dst = test_dir.join("renamed.txt");

    {
        let mut f = File::create(&src).expect("failed to create file");
        f.write_all(b"content").expect("failed to write");
    }

    let result = fs::rename(&src, &dst);
    assert!(result.is_ok(), "RENAME FILE FAILED: {:?}", result.err());
    assert!(!src.exists(), "source should not exist");
    assert!(dst.exists(), "destination should exist");

    eprintln!("    Rename file successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_copy_file() {
    print_test_header("test_copy_file");
    let test_dir = create_test_dir("copy_file");
    let src = test_dir.join("source.txt");
    let dst = test_dir.join("copy.txt");

    {
        let mut f = File::create(&src).expect("failed to create file");
        f.write_all(b"content to copy").expect("failed to write");
    }

    let result = fs::copy(&src, &dst);
    assert!(result.is_ok(), "COPY FILE FAILED: {:?}", result.err());
    assert!(src.exists(), "source should still exist");
    assert!(dst.exists(), "destination should exist");

    let content = fs::read_to_string(&dst).unwrap();
    assert_eq!(content, "content to copy");

    eprintln!("    Copy file successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_symlink() {
    print_test_header("test_symlink");
    let test_dir = create_test_dir("symlink");
    let target = test_dir.join("target.txt");
    let link = test_dir.join("link.txt");

    {
        let mut f = File::create(&target).expect("failed to create target");
        f.write_all(b"target content").expect("failed to write");
    }

    let result = symlink(&target, &link);
    assert!(result.is_ok(), "SYMLINK FAILED: {:?}", result.err());

    // verify symlink works
    let content = fs::read_to_string(&link).unwrap();
    assert_eq!(content, "target content");

    eprintln!("    Symlink successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// PERMISSION OPERATIONS
// ============================================================================

#[test]
fn test_chmod() {
    print_test_header("test_chmod");
    let test_dir = create_test_dir("chmod");
    let file_path = test_dir.join("chmoded.txt");

    File::create(&file_path).expect("failed to create file");

    let original_mode = fs::metadata(&file_path).unwrap().mode() & 0o7777;
    eprintln!("    Original mode: {:o}", original_mode);

    // chmod to 755
    let new_perms = fs::Permissions::from_mode(0o755);
    let result = fs::set_permissions(&file_path, new_perms);
    assert!(result.is_ok(), "CHMOD FAILED: {:?}", result.err());

    let new_mode = fs::metadata(&file_path).unwrap().mode() & 0o7777;
    eprintln!("    New mode: {:o}", new_mode);

    // verify mode actually changed (may be no-op on NFS with root_squash)
    assert_eq!(new_mode, 0o755, "CHMOD DID NOT TAKE EFFECT: mode is {:o}, expected 755", new_mode);

    eprintln!("    Chmod successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_chown() {
    print_test_header("test_chown");
    let test_dir = create_test_dir("chown");
    let file_path = test_dir.join("chowned.txt");

    File::create(&file_path).expect("failed to create file");

    let original_uid = fs::metadata(&file_path).unwrap().uid();
    let current_uid = unsafe { libc::getuid() };
    eprintln!("    Original file uid: {}, current uid: {}", original_uid, current_uid);

    // try to chown to current user (should work if we own the file)
    let result = unsafe {
        let path_cstr = std::ffi::CString::new(file_path.to_str().unwrap()).unwrap();
        libc::chown(path_cstr.as_ptr(), current_uid, current_uid as libc::gid_t)
    };

    assert!(result == 0, "CHOWN FAILED: {}", std::io::Error::last_os_error());

    let new_uid = fs::metadata(&file_path).unwrap().uid();
    assert_eq!(new_uid, current_uid, "CHOWN DID NOT TAKE EFFECT: uid is {}, expected {}", new_uid, current_uid);

    eprintln!("    Chown successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// ATOMIC WRITE WORKAROUND
// ============================================================================

#[test]
fn test_atomic_write_workaround() {
    print_test_header("test_atomic_write_workaround");
    let test_dir = create_test_dir("atomic_write");
    let file_path = test_dir.join("target.txt");
    let tmp_path = test_dir.join("target.txt.tmp");

    // create original file
    {
        let mut f = File::create(&file_path).expect("failed to create original");
        f.write_all(b"original content").expect("failed to write");
    }

    // atomic write: write to temp, then rename
    {
        let mut f = File::create(&tmp_path).expect("failed to create temp file");
        f.write_all(b"new content via atomic write").expect("failed to write temp");
    }

    let result = fs::rename(&tmp_path, &file_path);
    assert!(result.is_ok(), "ATOMIC RENAME FAILED: {:?}", result.err());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "new content via atomic write");

    eprintln!("    Atomic write workaround successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_atomic_append_workaround() {
    print_test_header("test_atomic_append_workaround");
    let test_dir = create_test_dir("atomic_append");
    let file_path = test_dir.join("appendable.txt");
    let tmp_path = test_dir.join("appendable.txt.tmp");

    // create original file
    {
        let mut f = File::create(&file_path).expect("failed to create original");
        f.write_all(b"line1\n").expect("failed to write");
    }

    // read original content
    let original = fs::read_to_string(&file_path).expect("failed to read original");

    // write to temp with appended content
    {
        let mut f = File::create(&tmp_path).expect("failed to create temp");
        f.write_all(original.as_bytes()).expect("failed to write original to temp");
        f.write_all(b"line2 (appended)\n").expect("failed to write appended");
    }

    // atomic rename
    let result = fs::rename(&tmp_path, &file_path);
    assert!(result.is_ok(), "ATOMIC APPEND RENAME FAILED: {:?}", result.err());

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("line1"), "original content missing");
    assert!(content.contains("line2 (appended)"), "appended content missing");

    eprintln!("    Atomic append workaround successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// DD-STYLE OPERATIONS
// ============================================================================

#[test]
fn test_dd_write_new_file() {
    print_test_header("test_dd_write_new_file");
    let test_dir = create_test_dir("dd_new");
    let file_path = test_dir.join("dd_output.bin");

    // write 1MB of data
    let data = vec![0u8; 1024 * 1024];

    let result = fs::write(&file_path, &data);
    assert!(result.is_ok(), "DD WRITE FAILED: {:?}", result.err());

    let meta = fs::metadata(&file_path).unwrap();
    assert_eq!(meta.len(), 1024 * 1024);

    eprintln!("    DD write 1MB successful");
    cleanup_test_dir(&test_dir);
}

#[test]
fn test_dd_read() {
    print_test_header("test_dd_read");
    let test_dir = create_test_dir("dd_read");
    let file_path = test_dir.join("dd_input.bin");

    // create 1MB file
    let data = vec![0xABu8; 1024 * 1024];
    fs::write(&file_path, &data).expect("failed to create test file");

    // read it back
    let result = fs::read(&file_path);
    assert!(result.is_ok(), "DD READ FAILED: {:?}", result.err());
    assert_eq!(result.unwrap().len(), 1024 * 1024);

    eprintln!("    DD read 1MB successful");
    cleanup_test_dir(&test_dir);
}

// ============================================================================
// ENVIRONMENT INFO (runs first due to name)
// ============================================================================

/// get runtime class from env (gvisor, kata, native)
fn get_runtime_class() -> String {
    std::env::var("RUNTIME_CLASS")
        .unwrap_or_else(|_| "unknown".to_string())
        .to_lowercase()
}

#[test]
fn test_00_environment_info() {
    print_test_header("ENVIRONMENT INFO");

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let euid = unsafe { libc::geteuid() };
    let egid = unsafe { libc::getegid() };

    eprintln!("    UID: {}, GID: {}", uid, gid);
    eprintln!("    EUID: {}, EGID: {}", euid, egid);

    // get runtime from env
    let runtime = get_runtime_class();
    eprintln!("    RUNTIME_CLASS: {}", runtime);

    if runtime == "unknown" {
        eprintln!("    WARNING: RUNTIME_CLASS not set. Set it to: gvisor, kata, or native");
    }

    // check kernel for info
    if let Ok(output) = Command::new("uname").arg("-r").output() {
        let kernel = String::from_utf8_lossy(&output.stdout);
        eprintln!("    Kernel: {}", kernel.trim());
    }

    // check NFS mount
    let test_path = test_base_path();
    eprintln!("    NFS_TEST_PATH: {}", test_path.display());
    eprintln!("    NFS path exists: {}", test_path.exists());

    if !test_path.exists() {
        panic!("NFS_TEST_PATH does not exist: {}. Set NFS_TEST_PATH env var.", test_path.display());
    }
}
