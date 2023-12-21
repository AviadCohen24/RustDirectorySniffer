use serde::{Serialize, Deserialize};
use std::{path::PathBuf, ffi::{CString, CStr}, os::raw::c_char, sync::{Arc, Mutex}, io};
use tokio::{fs, runtime::Runtime};
use lazy_static::lazy_static;
use async_recursion::async_recursion;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct FolderHierarchy {
    value: u64,
    name: String,
    path: String,
    children: Vec<FolderHierarchy>,
}

lazy_static! {
    static ref GLOBAL_DIRECTORY_MAP: Arc<Mutex<FolderHierarchy>> = Arc::new(Mutex::new(FolderHierarchy {
        value: 0,
        name: String::new(),
        path: String::new(),
        children: vec![],
    }));
}

#[async_recursion]
async fn get_folder_size(dir_path: &PathBuf) -> io::Result<u64> {
    let mut size: u64 = 0;
    let mut entries = fs::read_dir(dir_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            size += get_folder_size(&path).await?;
        } else {
            match path.metadata() {
                Ok(metadata) => size += metadata.len(),
                Err(e) => eprintln!("Failed to read metadata for {:?}: {}", path, e),
            }
        }
    }

    Ok(size)
}

#[async_recursion]
async fn scan_folder(directory_path: PathBuf, shared_state: Arc<Mutex<FolderHierarchy>>) -> io::Result<FolderHierarchy> {
    let mut entries = fs::read_dir(directory_path.clone()).await?;
    let mut children = Vec::new();
    let mut total_size = 0;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            let child_hierarchy = scan_folder(path, Arc::clone(&shared_state)).await?;
            total_size += child_hierarchy.value;
            children.push(child_hierarchy);
        } else {
            match path.metadata() {
                Ok(metadata) => total_size += metadata.len(),
                Err(e) => eprintln!("Failed to read metadata for {:?}: {}", path, e),
            }
        }
    }

    let name = directory_path.file_name().unwrap_or_default().to_str().unwrap_or("").to_string();
    let path = directory_path.to_str().unwrap_or("").to_string();

    Ok(FolderHierarchy {
        value: total_size,
        name,
        path,
        children,
    })
}

#[no_mangle]
pub extern "C" fn scan_directory_async(path_ptr: *const c_char) {
    let c_str = unsafe { CStr::from_ptr(path_ptr) };
    let path_str = match c_str.to_str() {
        Ok(str) => str,
        Err(_) => {
            eprintln!("Invalid string passed to scan_directory_async");
            return;
        }
    };
    let directory_path = PathBuf::from(path_str);

    let global_map_clone = Arc::clone(&GLOBAL_DIRECTORY_MAP);
    std::thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        match runtime.block_on(scan_folder(directory_path, global_map_clone)) {
            Ok(_) => println!("Scanning complete."),
            Err(e) => eprintln!("Scanning failed: {}", e),
        }
    });
}

#[no_mangle]
pub extern "C" fn get_directory_map(path_ptr: *const c_char) -> *mut c_char {
    let c_str = unsafe { CStr::from_ptr(path_ptr) };
    let path_str = match c_str.to_str() {
        Ok(str) => str,
        Err(_) => {
            eprintln!("Invalid string passed to get_directory_map");
            return std::ptr::null_mut();
        }
    };
    let directory_map = GLOBAL_DIRECTORY_MAP.lock().unwrap();

    let first_level = directory_map.children.iter()
        .filter(|child| child.path == path_str)
        .map(|child| FolderHierarchy {
            value: child.value,
            name: child.name.clone(),
            path: child.path.clone(),
            children: vec![], // Only return the first level
        })
        .collect::<Vec<_>>();

    let json = match serde_json::to_string(&first_level) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("Failed to serialize directory map: {}", e);
            return std::ptr::null_mut();
        }
    };

    CString::new(json).unwrap().into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir; 

    #[tokio::test]
    async fn test_get_folder_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(file_path).unwrap();
        writeln!(file, "Hello, world!").unwrap();

        let size = get_folder_size(&dir.path().to_path_buf()).await.unwrap();
        assert!(size > 0, "Folder size should be greater than 0");
    }

    #[tokio::test]
    async fn test_scan_folder() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_file.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Hello, world!").unwrap();

        let folder_hierarchy = scan_folder(dir.path().to_path_buf(), Arc::clone(&GLOBAL_DIRECTORY_MAP)).await.unwrap();
        println!("Folder Hierarchy: {:?}", folder_hierarchy);

        let locked_state = GLOBAL_DIRECTORY_MAP.lock().unwrap();
        println!("Locked state: {:?}", locked_state);
        
        assert_eq!(folder_hierarchy.name, "test", "The directory name should be 'test'.");
        assert_eq!(folder_hierarchy.value, 14, "The directory size should be 14 bytes.");
    }

    async fn create_test_directory_structure(base_dir: &PathBuf) -> io::Result<()> {
        fs::create_dir_all(base_dir.join("subfolder1/subsubfolder1")).await;
        fs::create_dir_all(base_dir.join("subfolder2")).await;
        fs::create_dir_all(base_dir.join("subfolder2/subsubfolder2a")).await;
        fs::create_dir_all(base_dir.join("subfolder2/subsubfolder2b")).await;

        let mut file1 = File::create(base_dir.join("subfolder1/test_file1.txt"))?;
        let mut file2 = File::create(base_dir.join("subfolder1/subsubfolder1/test_file2.txt"))?;
        let mut file3 = File::create(base_dir.join("subfolder2/test_file3.txt"))?;

        writeln!(file1, "Hello, world!")?;
        writeln!(file2, "Hello, Rust!")?;
        writeln!(file3, "Hello, Testing!")?;

        Ok(())
    }

    #[tokio::test]
    async fn test_scan_complex_folder() {
        let dir = tempdir().unwrap();
        create_test_directory_structure(&dir.path().to_path_buf()).await.unwrap();

        let folder_hierarchy = scan_folder(dir.path().to_path_buf(), Arc::clone(&GLOBAL_DIRECTORY_MAP)).await.unwrap();

        println!("Folder Hierarchy: {:?}", folder_hierarchy);

        // Assertions based on the structure you created
        assert_eq!(folder_hierarchy.children.len(), 2, "There should be two subfolders.");
    }

    #[tokio::test]
    async fn test_get_directory_map() {
        let dir = tempdir().unwrap();
        create_test_directory_structure(&dir.path().to_path_buf()).await.unwrap();

        
    }

}

fn main() {
    // This function is a placeholder and won't be used when called from TypeScript.
}
