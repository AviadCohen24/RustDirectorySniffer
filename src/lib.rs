use serde::{Serialize, Deserialize};
use std::{path::{PathBuf, Path}, ffi::{CString, CStr}, os::raw::c_char, sync::{Arc, Mutex, Condvar}};
use tokio::{fs, runtime::Runtime, io};
use async_recursion::async_recursion;
use event_listener::Event;

// A structure to represent the hierarchy of a folder with metadata.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct FolderHierarchy {
    value: u64,
    name: String,
    path: String,
    children: Vec<FolderHierarchy>,
}

// Events to signal the start, completion, and occurrence of errors during scanning.
struct ScanEvent {
    start: Event,
    complete: Event,
    error: Event,
}

impl ScanEvent {
    fn new() -> Self {
        Self {
            start: Event::new(),
            complete: Event::new(),
            error: Event::new(),
        }
    }
}

// Main structure to handle directory scanning with events and shared state.
pub struct DirectoryScanner {
    directory_map: Arc<Mutex<FolderHierarchy>>,
    stop_requested: Arc<(Mutex<bool>, Condvar)>,
    events: ScanEvent,
}

impl DirectoryScanner {
    // Constructor to initialize the DirectoryScanner with default values and events.
    fn new() -> Self {
        Self {
            directory_map: Arc::new(Mutex::new(FolderHierarchy::default())),
            stop_requested: Arc::new((Mutex::new(false), Condvar::new())),
            events: ScanEvent::new(),
        }
    }

    // Request to stop the directory scanning.
    fn request_stop(&self) {
        let (lock, cvar) = &*self.stop_requested;
        let mut stop = lock.lock().expect("Lock poisoned");
        *stop = true;
        cvar.notify_one();  // Notify the scanning process to stop.
    }

    // Check if a stop has been requested to terminate scanning early.
    fn is_stop_requested(&self) -> bool {
        let (lock, _) = &*self.stop_requested;
        *lock.lock().expect("Lock poisoned")
    }
}

impl Drop for DirectoryScanner {
    // Cleanup when the DirectoryScanner is dropped.
    fn drop(&mut self) {
        println!("Scanner is closing...");
    }
}

// Asynchronous recursive function to scan a directory and its subdirectories.
#[async_recursion]
async fn scan_folder(directory_path: PathBuf, scanner: Arc<DirectoryScanner>) -> io::Result<FolderHierarchy> {
    let mut entries = fs::read_dir(&directory_path).await?;
    let mut children = Vec::new();
    let mut total_size = 0;

    // Notify that scanning has started.
    scanner.events.start.notify(usize::MAX);

    while let Some(entry) = entries.next_entry().await? {
        // Check and handle if a stop request has been made.
        if scanner.is_stop_requested() {
            println!("Scanning stopped by request.");
            scanner.events.complete.notify(usize::MAX);  // Notify that scanning is complete due to stop request.
            return Ok(FolderHierarchy::default());  // Return an empty hierarchy as the scanning was stopped.
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

    // Notify that scanning of this directory is complete.
    scanner.events.complete.notify(usize::MAX);

    Ok(FolderHierarchy {
        value: total_size,
        name,
        path,
        children,
    })
}

// FFI functions to interact with the scanner from other languages like C.

// Create a new instance of DirectoryScanner.
#[no_mangle]
pub extern "C" fn create_directory_scanner() -> *mut DirectoryScanner {
    let scanner = Box::new(DirectoryScanner::new());
    Box::into_raw(scanner)  // Return a raw pointer to the scanner for use in FFI.
}

// Free the memory allocated for DirectoryScanner.
#[no_mangle]
pub extern "C" fn free_directory_scanner(scanner_ptr: *mut DirectoryScanner) {
    if !scanner_ptr.is_null() {
        unsafe { Box::from_raw(scanner_ptr) };  // Convert the raw pointer back to Box and drop it.
    }
}

// Start scanning a directory asynchronously.
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

    // Spawn a new thread to handle asynchronous scanning.
    tokio::spawn(async move {
        let runtime = Runtime::new().unwrap();
        runtime.block_on(async {
            let root_hierarchy = FolderHierarchy {
                value: 0, 
                name: directory_path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                path: directory_path.to_string_lossy().into_owned(),
                children: vec![],
            };
            let mut directory_map = directory_map_clone.lock().unwrap();
            *directory_map = root_hierarchy;

            // Continue scanning the directory and its subdirectories.
            while let Some(entry) = fs::read_dir(directory_path.clone()).await.unwrap().next_entry().await.unwrap() {
                let path = entry.path();

                if path.is_dir() {
                    let sub_hierarchy = scan_folder(path, Arc::clone(&scanner_clone)).await.unwrap();
                    directory_map.value += sub_hierarchy.value;
                    directory_map.children.push(sub_hierarchy);
                } else if let Ok(metadata) = path.metadata() {
                    directory_map.value += metadata.len();
                    let file_entry = FolderHierarchy {
                        value: metadata.len(),
                        name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                        path: path.parent().unwrap_or_else(|| Path::new("")).to_string_lossy().into_owned(),
                        children: vec![],
                    };
                    directory_map.children.push(file_entry);
                }
            }
        });
    });
}

// Retrieve the current state of the directory map as a JSON object.
#[no_mangle]
pub extern "C" fn get_directory_map(scanner_ptr: *const Arc<DirectoryScanner>, path_ptr: *const c_char, depth: i32) -> *mut c_char {
    let scanner = unsafe {
        assert!(!scanner_ptr.is_null(), "Scanner pointer is null.");
        (&*scanner_ptr).clone()  // Safely clone the Arc to get a new reference to the same DirectoryScanner.
    };

    let path_str = unsafe {
        assert!(!path_ptr.is_null(), "Path pointer is null.");
        CStr::from_ptr(path_ptr)
            .to_str()
            .expect("Invalid UTF-8 in path")
            .replace("\\", "/")
    };

    // Acquire the lock and clone the current state of the directory map.
    let directory_map = {
        let lock = scanner.directory_map.lock().unwrap();
        lock.clone()
    };

    // Process the directory_map to generate a JSON object representing the current scan state.
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
        serde_json::to_string(&hierarchy).unwrap_or_else(|e| format!("error: Serialization error {e}"))
    } else {
        format!("{{'error': 'Root folder not found'}}")
    };

    // Return the JSON object as a C string.
    CString::new(json).unwrap().into_raw()
}

// Allow external request to stop the ongoing scanning.
pub extern "C" fn stop_scanning(scanner_ptr: *const DirectoryScanner) {
    if scanner_ptr.is_null() {
        eprintln!("Scanner pointer is null.");
        return;
    }

    let scanner = unsafe { &*scanner_ptr };
    scanner.request_stop();
}

// --------------------------unit tests--------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir::TempDir;
    use std::fs::{self, File};
    use std::io::Write;
    use tokio::sync::mpsc;

    // Asynchronous scanning test to ensure the scanning function works asynchronously.
    #[tokio::test]
    async fn test_asynchronous_scanning() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let scanner = Arc::new(DirectoryScanner::new());

        let scan_result = scan_folder(temp_path.clone(), scanner.clone()).await;
        assert!(scan_result.is_ok(), "Scan should complete successfully.");
    }

    // Test to ensure the retrieval of the directory map is accurate.
    #[tokio::test]
    async fn test_get_directory_map() {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let scanner = Arc::new(DirectoryScanner::new());

        let _ = scan_folder(temp_path.clone(), scanner.clone()).await;

        let directory_map = scanner.directory_map.lock().unwrap();
        assert_eq!(directory_map.children.len(), 0, "Directory map should initially be empty.");
    }

    // Complex folder scanning test to ensure functions aren't blocking and can execute asynchronously.
    #[tokio::test]
    async fn test_complex_folder_scanning() {
        let temp_dir = TempDir::new("complex_test_dir").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        // Create a complex directory structure
        for i in 0..2 {
            let subdir_path = temp_path.join(format!("subdir_{}", i));
            fs::create_dir(&subdir_path).unwrap();
            for j in 0..5 {
                let file_path = subdir_path.join(format!("file_{}.txt", j));
                let mut file = File::create(&file_path).unwrap();
                writeln!(file, "This is a test file.").unwrap();
            }
        }
        println!("Create directory successed");

        let scanner = Arc::new(DirectoryScanner::new());
        let (tx, mut rx) = mpsc::channel(1);

        let (testPath_ptr, testPath_cstring) = convert_pathbuf_to_c_char_pointer(temp_path)
        .expect("Failed to convert path");

        println!("Start scanning in separate task");
        // Clone the scanner for use in the separate task
        let scanner_clone = scanner.clone();
        // Spawn the scanning in a separate task
        tokio::spawn(async move {
            scan_directory_async(&scanner_clone, testPath_cstring.as_ptr());
            tx.send(()).await.unwrap();
        });
        
        println!("Trying to retrieve the directory map while scanning");
        // While scanning, try to retrieve the directory map
        {
            let directory_map = scanner.directory_map.lock().unwrap();
            assert!(!directory_map.children.is_empty(), "Directory map should not be empty during scanning.");
        }        

        // Wait for the scanning to complete
        rx.recv().await;

        println!("Scanning completed");
        // Check the directory map after scanning is complete
        let directory_map = scanner.directory_map.lock().unwrap();
        println!("\nThe directory map afetr scanning is:\n{:?}\n", directory_map);
        assert!(!directory_map.children.is_empty(), "Directory map should not be empty after scanning.");
    }

    fn convert_pathbuf_to_c_char_pointer(path: PathBuf) -> Result<(*const c_char, CString), std::ffi::NulError> {
        // Convert PathBuf to String
        let path_str = path.into_os_string().into_string().expect("Path contains invalid Unicode");
    
        // Create a CString
        let c_str = CString::new(path_str)?;
    
        // Obtain a pointer to the C string
        let ptr = c_str.as_ptr();
    
        // Return both the pointer and the CString to manage its lifetime
        Ok((ptr, c_str))
    }
}
