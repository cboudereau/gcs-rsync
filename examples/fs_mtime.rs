use filetime::FileTime;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), ()> {
    {
        let mut fs = tokio::fs::File::create("test.txt").await.unwrap();

        fs.write_all(b"hello").await.unwrap();
        drop(fs);
    }

    {
        let fs = tokio::fs::File::open("test.txt").await.unwrap();

        let mtime = FileTime::from_unix_time(0, 0);
        filetime::set_file_mtime("test.txt", mtime).unwrap();

        fs.sync_all().await.unwrap();
        let metadata = fs.metadata().await.unwrap();
        let modified = metadata.modified().unwrap();
        println!("modified: {:?}", modified);
        let created = metadata.created().unwrap();
        println!("created: {:?}", created);
    }

    Ok(())
}
