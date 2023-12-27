use serde::{Serialize, Deserialize};
use std::{path::{PathBuf, Path}, ffi::{CString, CStr}, os::raw::c_char, sync::{Arc, Mutex}};
use tokio::{fs, runtime::Runtime, io};
use async_recursion::async_recursion;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct FolderHierarchy {
    value: u64,
    name: String,
    path: String,
    children: Vec<FolderHierarchy>,
}

pub struct DirectoryScanner {
    directory_map: Arc<Mutex<FolderHierarchy>>,
    stop_requested: Arc<Mutex<bool>>,
}

impl DirectoryScanner {
    fn new() -> Self {
        Self {
            directory_map: Arc::new(Mutex::new(FolderHierarchy::default())),
            stop_requested: Arc::new(Mutex::new(false)),
        }
    }

    fn request_stop(&self) {
        let mut stop = self.stop_requested.lock().expect("Lock poisoned");
        *stop = true;
    }

    fn is_stop_requested(&self) -> bool {
        *self.stop_requested.lock().expect("Lock poisoned")
    }
}

impl Drop for DirectoryScanner {
    fn drop(&mut self) {
        println!("Scanner is closing...");
    }
}

#[async_recursion]
async fn scan_folder(directory_path: PathBuf, scanner: Arc<DirectoryScanner>) -> io::Result<FolderHierarchy> {
    let mut entries = fs::read_dir(&directory_path).await?;
    let mut children = Vec::new();
    let mut total_size = 0;

    while let Some(entry) = entries.next_entry().await? {
        if scanner.is_stop_requested() {
            println!("Scanning stopped by request.");
            return Ok(FolderHierarchy::default());
        }

        let path = entry.path();
        if path.is_dir() {
            let child_hierarchy = scan_folder(path, Arc::clone(&scanner)).await?;
            total_size += child_hierarchy.value;
            children.push(child_hierarchy);
        } else if let Ok(metadata) = path.metadata() {
            total_size += metadata.len();
        }
    }

    let name = directory_path.file_name()
                  .and_then(|n| n.to_str())
                  .unwrap_or("")
                  .to_string();
    let path = directory_path.to_string_lossy().into_owned();

    Ok(FolderHierarchy {
        value: total_size,
        name,
        path,
        children,
    })
}

#[no_mangle]
pub extern "C" fn create_directory_scanner() -> *mut DirectoryScanner {
    let scanner = DirectoryScanner {
        directory_map: Arc::new(Mutex::new(FolderHierarchy::default())),
        stop_requested: Arc::new(Mutex::new(false)),
    };

    let arc = Arc::new(scanner);

    Arc::into_raw(arc) as *mut DirectoryScanner
}

#[no_mangle]
pub extern "C" fn free_directory_scanner(scanner_ptr: *mut DirectoryScanner) {
    // Safety: Ensure the provided pointer is valid and not null
    if !scanner_ptr.is_null() {
        // Convert the raw pointer back to an Arc, which will be dropped at the end of this scope
        // Dropping the last Arc will free the DirectoryScanner
        unsafe { Arc::from_raw(scanner_ptr) };
    }
}


#[no_mangle]
pub extern "C" fn scan_directory_async(scanner_ptr: *const Arc<DirectoryScanner>, path_ptr: *const c_char) {
    let scanner = unsafe {
        assert!(!scanner_ptr.is_null(), "Scanner pointer is null.");
        &*scanner_ptr
    };

    let c_str = unsafe { CStr::from_ptr(path_ptr) };
    let path_str = match c_str.to_str() {
        Ok(str) => str,
        Err(_) => {
            eprintln!("Invalid string passed to scan_directory_async");
            return;
        }
    };
    let directory_path = PathBuf::from(path_str);

    let directory_map_clone = Arc::clone(&scanner.directory_map);

    let scanner_clone = Arc::clone(scanner);

    std::thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        runtime.block_on(async {
            let root_hierarchy = FolderHierarchy {
                value: 0, 
                name: directory_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                path: directory_path.to_string_lossy().into_owned(),
                children: vec![],
            };
            let mut entries = fs::read_dir(directory_path.clone()).await.unwrap();

            let mut directory_map = directory_map_clone.lock().unwrap();
            *directory_map = root_hierarchy;

            while let Some(entry) = entries.next_entry().await.unwrap() {
                let path = entry.path();

                if path.is_dir() {
                    let sub_hierarchy = scan_folder(path, Arc::clone(&scanner_clone)).await.unwrap();
                    directory_map.value += sub_hierarchy.value;
                    directory_map.children.push(sub_hierarchy);
                } else {
                    match path.metadata() {
                        Ok(metadata) => {
                            directory_map.value += metadata.len();
                            let file_entry = FolderHierarchy {
                                value: metadata.len(),
                                name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                                path: path.parent().unwrap_or_else(|| Path::new("")).to_string_lossy().into_owned(),
                                children: vec![],
                            };
                            directory_map.children.push(file_entry);
                        },
                        Err(e) => eprintln!("Failed to read metadata for {:?}: {}", path, e),
                    }
                }
            }
        });
    });
}

#[no_mangle]
pub extern "C" fn get_directory_map(scanner_ptr: *const DirectoryScanner, path_ptr: *const c_char, depth: i32) -> *mut c_char {
    let scanner = unsafe {
        assert!(!scanner_ptr.is_null(), "Scanner pointer is null.");
        &*scanner_ptr
    };

    let path_str = unsafe {
        assert!(!path_ptr.is_null(), "Path pointer is null.");
        CStr::from_ptr(path_ptr)
            .to_str()
            .expect("Invalid UTF-8 in path")
            .replace("\\", "/")
    };

    // Attempt to acquire the lock.
    let guard = match scanner.directory_map.lock() {
        Ok(g) => g,
        Err(e) => {
            // Handle lock poisoning or other errors.
            eprintln!("Failed to lock directory_map: {}", e);
            return CString::new("{\"error\":\"internal error\"}").unwrap().into_raw();
        }
    };

    // Quickly clone the data needed and release the lock.
    let directory_map = (*guard).clone();

    // Process the directory_map to generate JSON.
    let json = if directory_map.path.replace("\\", "/") == path_str {
        let hierarchy = match depth {
            0 => FolderHierarchy {
                value: directory_map.value,
                name: directory_map.name.clone(),
                path: directory_map.path.clone(),
                children: directory_map.children.iter().map(|child| FolderHierarchy {
                    value: child.value,
                    name: child.name.clone(),
                    path: child.path.clone(),
                    children: vec![],
                }).collect(),
            },
            1 => directory_map.clone(),
            _ => FolderHierarchy::default(),
        };
        serde_json::to_string(&hierarchy).unwrap_or_else(|e| format!("{{\"error\": \"Serialization error: {}\"}}", e))
    } else {
        format!("{{\"error\": \"Root folder not found\"}}")
    };

    CString::new(json).unwrap().into_raw()
}

pub extern "C" fn stop_scanning(scanner_ptr: *const DirectoryScanner) {
    if scanner_ptr.is_null() {
        eprintln!("Scanner pointer is null.");
        return;
    }

    let scanner = unsafe { &*scanner_ptr };
    scanner.request_stop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use std::ffi::{CString, CStr};
    use std::thread;
    use tokio::fs;
    use std::time::Duration;
    use std::sync::Arc;

    async fn create_test_directory_structure(base_dir: &PathBuf) -> io::Result<()> {
        fs::create_dir_all(base_dir.join("subfolder1/subsubfolder1")).await?;
        fs::create_dir_all(base_dir.join("subfolder2")).await?;
        fs::create_dir_all(base_dir.join("subfolder2/subsubfolder2a")).await?;
        fs::create_dir_all(base_dir.join("subfolder2/subsubfolder2b")).await?;
    
        let mut file1 = File::create(base_dir.join("subfolder1/test_file1.txt"))?;
        let mut file2 = File::create(base_dir.join("subfolder1/subsubfolder1/test_file2.txt"))?;
        let mut file3 = File::create(base_dir.join("subfolder2/test_file3.txt"))?;
    
        writeln!(file1, "Hello, world!")?;
        writeln!(file2, "Hello, Rust!")?;
        writeln!(file3, "Hello, Testing!")?;
    
        Ok(())
    }
    
    #[tokio::test]
    async fn test_scan_and_get_directory_map() {
        let temp_dir = tempdir().expect("Failed to create a temporary directory");
        let test_path = temp_dir.path();

        create_test_directory_structure(&temp_dir.path().to_path_buf()).await.unwrap();

        let scanner = Arc::new(DirectoryScanner::new());

        let scanner_arc = &scanner as *const _;

        let test_path_c = CString::new(test_path.to_str().unwrap()).expect("CString::new failed");

        let scanner_ptr = Arc::into_raw(scanner);

        scan_directory_async(scanner_arc, test_path_c.as_ptr());

        thread::sleep(Duration::from_millis(10)); // Adjust as necessary.

        let scanner = unsafe { Arc::from_raw(scanner_ptr) };

        let result_ptr = get_directory_map(&*scanner, test_path_c.as_ptr(), 0);
        assert!(!result_ptr.is_null(), "get_directory_map returned a null pointer");

        let result_cstr = unsafe { CStr::from_ptr(result_ptr) };
        let result_str = result_cstr.to_str().unwrap();

        let directory_map: FolderHierarchy = serde_json::from_str(result_str).unwrap();

        assert!(!directory_map.children.is_empty(), "The directory map childrens should not be empty");
        
        for folder in &directory_map.children {
            assert!(
                folder.children.iter().all(|child| child.children.is_empty()),
                "All children of the folder should have an empty children array"
            );
        }
    }
 
}


fn main() {
    // This function is a placeholder and won't be used when called from TypeScript.
}
