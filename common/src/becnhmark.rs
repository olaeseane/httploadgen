use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

// only for becnhmarks
pub async fn do_request_raw(mut client: TcpStream, total_requests: u64) {
    let req: &[u8] = b"GET /rate HTTP/1.1\nhost: 127.0.0.1:8080\n\n";

    let mut read_buf = vec![0u8; 8196];
    for _ in 0..total_requests {
        client.write_all(req).await.unwrap();
        let _response_code = match client.read(&mut read_buf).await {
            Ok(n) => {
                if n == 0 {
                    Err("Empty read")
                } else {
                    let s = std::str::from_utf8(&read_buf[..n]).unwrap();
                    match s
                        .lines()
                        .filter(|l| l.starts_with("HTTP/1.1"))
                        .filter_map(|s| s.split_whitespace().nth(1))
                        .filter_map(|n| n.parse::<u16>().ok())
                        .next()
                    {
                        Some(l) => Ok(l),
                        None => Err("Parsing error"),
                    }
                }
            }
            Err(_) => Err("Connection error"), // something might have gone wrong, we ignore that for now
        };
    }
}
