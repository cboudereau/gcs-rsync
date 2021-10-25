use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() {
    let mut fs = tokio::fs::File::create("test.txt").await.unwrap();
    fs.write_all(b"hello").await.unwrap();
}
