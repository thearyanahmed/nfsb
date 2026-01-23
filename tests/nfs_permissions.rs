//! NFS Permission Tests for gVisor/Kata/Native runtimes
//!
//! Tests all file operations from the APPS-13266 test matrix.
//! Run as root: cargo test
//! Run as non-root: su nfsb -c "cargo test"
//!
//! Environment variable:
//!   NFS_TEST_PATH - path to NFS mount (default: /mnt/nfs)

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
    let euid = unsafe { libc::geteuid() };
    format!("uid={}, gid={}, euid={}", uid, gid, euid)
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
    assert!(result.is_ok(), "failed to create file: {:?}", result.err());

    // verify file exists
    assert!(file_path.exists(), "file should exist after creation");

    // check ownership
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
    assert!(result.is_ok(), "failed to create directory: {:?}", result.err());

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

    // this may fail for non-root users on gVisor
    // because first level is created as nobody-owned 755
    if result.is_err() {
        eprintln!("    EXPECTED FAILURE for non-root: {:?}", result.err());
    } else {
        assert!(nested_path.is_dir(), "nested directory should exist");
        eprintln!("    SUCCESS: nested directories created");
    }

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
    let result = fs::read_to_string(&file_path);
    assert!(result.is_ok(), "failed to read file: {:?}", result.err());
    assert_eq!(result.unwrap(), "test content for reading");
    eprintln!("    SUCCESS: file read correctly");

    cleanup_test_dir(&test_dir);
}

// ============================================================================
// MODIFY OPERATIONS - these fail for non-root on gVisor/Kata
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

    eprintln!("    File created, attempting append...");

    // try to append - this is where non-root fails on gVisor/Kata
    let result = OpenOptions::new()
        .append(true)
        .open(&file_path);

    match result {
        Ok(mut f) => {
            let write_result = f.write_all(b"appended content\n");
            if write_result.is_err() {
                eprintln!("    EXPECTED FAILURE (write after open): {:?}", write_result.err());
            } else {
                eprintln!("    SUCCESS: append operation completed");

                // verify content
                let content = fs::read_to_string(&file_path).unwrap();
                assert!(content.contains("appended content"), "appended content should be present");
            }
        }
        Err(e) => {
            eprintln!("    EXPECTED FAILURE (open for append): {}", e);
        }
    }

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

    eprintln!("    File created, attempting overwrite...");

    // try to overwrite - this fails for non-root on gVisor/Kata
    let result = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&file_path);

    match result {
        Ok(mut f) => {
            let write_result = f.write_all(b"new content");
            if write_result.is_err() {
                eprintln!("    EXPECTED FAILURE (write after open): {:?}", write_result.err());
            } else {
                eprintln!("    SUCCESS: overwrite operation completed");

                let content = fs::read_to_string(&file_path).unwrap();
                assert_eq!(content, "new content");
            }
        }
        Err(e) => {
            eprintln!("    EXPECTED FAILURE (open for write): {}", e);
        }
    }

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

    // try to create file in subdirectory
    // fails for non-root on gVisor because subdir is 755 owned by nobody
    let result = File::create(&file_in_subdir);

    match result {
        Ok(mut f) => {
            let write_result = f.write_all(b"content in subdir");
            if write_result.is_err() {
                eprintln!("    EXPECTED FAILURE (write): {:?}", write_result.err());
            } else {
                eprintln!("    SUCCESS: wrote file in subdirectory");
            }
        }
        Err(e) => {
            eprintln!("    EXPECTED FAILURE (create in subdir): {}", e);
        }
    }

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

    eprintln!("    File created (1KB), attempting seek+write...");

    // try to open, seek, and write - simulates dd with seek
    let result = OpenOptions::new()
        .write(true)
        .open(&file_path);

    match result {
        Ok(mut f) => {
            if let Err(e) = f.seek(SeekFrom::Start(512)) {
                eprintln!("    EXPECTED FAILURE (seek): {}", e);
            } else {
                let write_result = f.write_all(&[0xFFu8; 512]);
                if write_result.is_err() {
                    eprintln!("    EXPECTED FAILURE (write after seek): {:?}", write_result.err());
                } else {
                    eprintln!("    SUCCESS: seek+write completed");
                }
            }
        }
        Err(e) => {
            eprintln!("    EXPECTED FAILURE (open for write): {}", e);
        }
    }

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_touch_existing_file() {
    print_test_header("test_touch_existing_file");
    let test_dir = create_test_dir("touch_existing");
    let file_path = test_dir.join("touchable.txt");

    // create file
    File::create(&file_path).expect("failed to create file");

    let _original_mtime = fs::metadata(&file_path).unwrap().modified().unwrap();

    // small delay to ensure different timestamp
    std::thread::sleep(std::time::Duration::from_millis(100));

    // try to touch (update timestamps)
    let result = OpenOptions::new()
        .write(true)
        .open(&file_path);

    match result {
        Ok(f) => {
            // just opening for write and closing should update mtime on some systems
            drop(f);

            // alternatively use filetime crate or utime syscall
            // for now just check if open succeeded
            eprintln!("    SUCCESS: opened existing file for write (touch)");
        }
        Err(e) => {
            eprintln!("    EXPECTED FAILURE (touch existing): {}", e);
        }
    }

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

    // delete depends on parent directory permissions (777), not file permissions
    let result = fs::remove_file(&file_path);
    assert!(result.is_ok(), "failed to delete file: {:?}", result.err());
    assert!(!file_path.exists(), "file should not exist after deletion");
    eprintln!("    SUCCESS: file deleted");

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_delete_directory() {
    print_test_header("test_delete_directory");
    let test_dir = create_test_dir("delete_dir");
    let subdir = test_dir.join("deletable_dir");

    fs::create_dir(&subdir).expect("failed to create directory");
    assert!(subdir.is_dir());

    // delete empty directory - depends on parent directory permissions
    let result = fs::remove_dir(&subdir);

    // may fail on Kata non-root
    if result.is_err() {
        eprintln!("    EXPECTED FAILURE (Kata non-root): {:?}", result.err());
    } else {
        assert!(!subdir.exists(), "directory should not exist after deletion");
        eprintln!("    SUCCESS: directory deleted");
    }

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

    // rename operates on parent directory, should work with 777 parent
    let result = fs::rename(&src, &dst);
    assert!(result.is_ok(), "failed to rename: {:?}", result.err());
    assert!(!src.exists(), "source should not exist");
    assert!(dst.exists(), "destination should exist");
    eprintln!("    SUCCESS: file renamed");

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

    // copy creates a new file, should work with 777 parent
    let result = fs::copy(&src, &dst);
    assert!(result.is_ok(), "failed to copy: {:?}", result.err());
    assert!(src.exists(), "source should still exist");
    assert!(dst.exists(), "destination should exist");

    let content = fs::read_to_string(&dst).unwrap();
    assert_eq!(content, "content to copy");
    eprintln!("    SUCCESS: file copied");

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

    // symlink creation depends on parent directory permissions
    let result = symlink(&target, &link);
    assert!(result.is_ok(), "failed to create symlink: {:?}", result.err());

    // verify symlink works
    let content = fs::read_to_string(&link).unwrap();
    assert_eq!(content, "target content");
    eprintln!("    SUCCESS: symlink created and readable");

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

    // try to chmod - fails for non-root (not owner of nobody-owned file)
    let new_perms = fs::Permissions::from_mode(0o755);
    let result = fs::set_permissions(&file_path, new_perms);

    match result {
        Ok(_) => {
            let new_mode = fs::metadata(&file_path).unwrap().mode() & 0o7777;
            eprintln!("    New mode: {:o}", new_mode);

            // check if it actually changed (may be silent no-op on NFS)
            if new_mode == 0o755 {
                eprintln!("    SUCCESS: chmod worked");
            } else {
                eprintln!("    SILENT NO-OP: chmod succeeded but mode unchanged (NFS root_squash)");
            }
        }
        Err(e) => {
            eprintln!("    EXPECTED FAILURE (chmod): {}", e);
        }
    }

    cleanup_test_dir(&test_dir);
}

#[test]
fn test_chown() {
    print_test_header("test_chown");
    let test_dir = create_test_dir("chown");
    let file_path = test_dir.join("chowned.txt");

    File::create(&file_path).expect("failed to create file");

    let original_uid = fs::metadata(&file_path).unwrap().uid();
    eprintln!("    Original uid: {}", original_uid);

    // try to chown to uid 1000 - requires root or being the owner
    // on NFS with root_squash, even root's chown may silently fail
    let result = unsafe {
        let path_cstr = std::ffi::CString::new(file_path.to_str().unwrap()).unwrap();
        libc::chown(path_cstr.as_ptr(), 1000, 1000)
    };

    if result == 0 {
        let new_uid = fs::metadata(&file_path).unwrap().uid();
        eprintln!("    New uid: {}", new_uid);

        if new_uid == 1000 {
            eprintln!("    SUCCESS: chown worked");
        } else {
            eprintln!("    SILENT NO-OP: chown succeeded but uid unchanged (NFS root_squash)");
        }
    } else {
        let errno = std::io::Error::last_os_error();
        eprintln!("    EXPECTED FAILURE (chown): {}", errno);
    }

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

    eprintln!("    Original file created");
    eprintln!("    Attempting atomic write workaround (temp + rename)...");

    // atomic write: write to temp, then rename
    // this SHOULD work even on gVisor/Kata non-root
    {
        let mut f = File::create(&tmp_path).expect("failed to create temp file");
        f.write_all(b"new content via atomic write").expect("failed to write temp");
    }

    let result = fs::rename(&tmp_path, &file_path);
    assert!(result.is_ok(), "atomic rename failed: {:?}", result.err());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "new content via atomic write");
    eprintln!("    SUCCESS: atomic write workaround works!");

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

    eprintln!("    Original file created");
    eprintln!("    Attempting atomic append workaround...");

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
    assert!(result.is_ok(), "atomic rename failed: {:?}", result.err());

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("line1"));
    assert!(content.contains("line2 (appended)"));
    eprintln!("    SUCCESS: atomic append workaround works!");

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

    // write 1MB of data (like dd if=/dev/zero of=file bs=1M count=1)
    let data = vec![0u8; 1024 * 1024];

    let result = fs::write(&file_path, &data);
    assert!(result.is_ok(), "dd write failed: {:?}", result.err());

    let meta = fs::metadata(&file_path).unwrap();
    assert_eq!(meta.len(), 1024 * 1024);
    eprintln!("    SUCCESS: wrote 1MB file");

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

    // read it back (like dd if=file of=/dev/null)
    let result = fs::read(&file_path);
    assert!(result.is_ok(), "dd read failed: {:?}", result.err());
    assert_eq!(result.unwrap().len(), 1024 * 1024);
    eprintln!("    SUCCESS: read 1MB file");

    cleanup_test_dir(&test_dir);
}

// ============================================================================
// ENVIRONMENT INFO
// ============================================================================

#[test]
fn test_print_environment_info() {
    print_test_header("ENVIRONMENT INFO");

    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let euid = unsafe { libc::geteuid() };
    let egid = unsafe { libc::getegid() };

    eprintln!("    UID: {}, GID: {}", uid, gid);
    eprintln!("    EUID: {}, EGID: {}", euid, egid);

    // get username
    if let Ok(user) = std::env::var("USER") {
        eprintln!("    USER: {}", user);
    }

    // check kernel
    if let Ok(output) = Command::new("uname").arg("-r").output() {
        let kernel = String::from_utf8_lossy(&output.stdout);
        eprintln!("    Kernel: {}", kernel.trim());
    }

    // check if running in gVisor (kernel 4.4.0)
    if let Ok(output) = Command::new("uname").arg("-r").output() {
        let kernel = String::from_utf8_lossy(&output.stdout);
        if kernel.contains("4.4.0") {
            eprintln!("    Runtime: gVisor (detected via kernel 4.4.0)");
        } else if kernel.contains("6.12") {
            eprintln!("    Runtime: Kata (detected via kernel 6.12.x)");
        } else {
            eprintln!("    Runtime: Native or unknown");
        }
    }

    // check NFS mount
    let test_path = test_base_path();
    eprintln!("    NFS test path: {}", test_path.display());

    if test_path.exists() {
        eprintln!("    NFS path exists: true");

        // try to get mount info
        if let Ok(output) = Command::new("mount").output() {
            let mounts = String::from_utf8_lossy(&output.stdout);
            for line in mounts.lines() {
                if line.contains(test_path.to_str().unwrap_or("")) || line.contains("nfs") {
                    eprintln!("    Mount: {}", line);
                }
            }
        }
    } else {
        eprintln!("    NFS path exists: false - tests may fail");
    }
}

// ============================================================================
// FULL TEST SUMMARY
// ============================================================================

#[test]
fn test_zz_summary() {
    // named zz_ to run last
    print_test_header("TEST SUMMARY");

    let uid = unsafe { libc::getuid() };
    let is_root = uid == 0;

    eprintln!();
    eprintln!("    Running as: {} (uid={})", if is_root { "ROOT" } else { "NON-ROOT" }, uid);
    eprintln!();
    eprintln!("    Expected results based on APPS-13266 findings:");
    eprintln!();

    if is_root {
        eprintln!("    | Operation              | Expected |");
        eprintln!("    |------------------------|----------|");
        eprintln!("    | Create file            | PASS     |");
        eprintln!("    | Read file              | PASS     |");
        eprintln!("    | Append to file         | PASS     |");
        eprintln!("    | Overwrite file         | PASS     |");
        eprintln!("    | Create directory       | PASS     |");
        eprintln!("    | Write in subdirectory  | PASS     |");
        eprintln!("    | Delete file            | PASS     |");
        eprintln!("    | Rename/Move            | PASS     |");
        eprintln!("    | Symlink                | PASS     |");
        eprintln!("    | chmod                  | PASS*    |");
        eprintln!("    | chown                  | NO-OP*   |");
        eprintln!("    | Atomic write workaround| PASS     |");
        eprintln!();
        eprintln!("    * On NFS with root_squash, chmod may work but chown silently fails");
    } else {
        eprintln!("    | Operation              | gVisor | Kata   | Native |");
        eprintln!("    |------------------------|--------|--------|--------|");
        eprintln!("    | Create file            | PASS   | PASS   | PASS   |");
        eprintln!("    | Read file              | PASS   | PASS   | PASS   |");
        eprintln!("    | Append to file         | FAIL   | FAIL   | PASS   |");
        eprintln!("    | Overwrite file         | FAIL   | FAIL   | PASS   |");
        eprintln!("    | Create directory       | PASS   | PASS   | PASS   |");
        eprintln!("    | Write in subdirectory  | FAIL   | PASS   | PASS   |");
        eprintln!("    | Nested mkdir -p        | FAIL   | PASS   | PASS   |");
        eprintln!("    | Delete file            | PASS   | PASS   | PASS   |");
        eprintln!("    | Delete directory       | PASS   | FAIL   | PASS   |");
        eprintln!("    | Rename/Move            | PASS   | PASS   | PASS   |");
        eprintln!("    | Symlink                | PASS   | PASS   | PASS   |");
        eprintln!("    | chmod                  | NO-OP  | FAIL   | PASS   |");
        eprintln!("    | chown                  | FAIL   | FAIL   | FAIL   |");
        eprintln!("    | dd append (seek)       | FAIL   | FAIL   | PASS   |");
        eprintln!("    | Atomic write workaround| PASS   | PASS   | PASS   |");
    }

    eprintln!();
    eprintln!("    To switch users:");
    eprintln!("      As root:     cargo test");
    eprintln!("      As non-root: su nfsb -c 'cargo test'");
    eprintln!();
}
