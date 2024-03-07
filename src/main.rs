use actix_files::{NamedFile, Files};
use tokio::{
    fs::{self, File}, io::{AsyncWriteExt}, select, task, time::*,
    sync::mpsc::*,
    process::*,
};
use actix_web::{
    *,
};
use actix_ws::{
    Message as WsMessage,
    MessageStream,
    Session
};
use serde::{Serialize, Deserialize};
use serde_json as json;
use rand::prelude::*;

use std::{
    collections::hash_map::DefaultHasher, error::Error, hash::*, net::SocketAddr, path::PathBuf, sync::atomic::{AtomicU64, Ordering}, io
};


const HEARTBEAT_TICK: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_secs(30);
// static UID: AtomicU64 = AtomicU64::new(0);


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    HttpServer::new(move || {
        App::new()
        .service(Files::new("/static", "./web"))
        .service(index)
        .service(download)
        .service(web::resource("/connect").route(web::get().to(connect)))
    })
    .bind(("127.0.0.1", 80))?
    .run()
    .await?;

    Ok(())
}

#[get("/")]
async fn index() -> io::Result<impl Responder> {
    NamedFile::open_async("./web/index.html").await
}

#[get("/d/{id}")]
async fn download(id: web::Path<String>) -> io::Result<impl Responder> {
    let err = io::Error::new(io::ErrorKind::PermissionDenied, "Invalid ID");

    if id.len() != 5 || id.contains(".") {
        return io::Result::Err(err);
    }
    let workspace = PathBuf::from(format!(".\\workspace\\{}", id));

    if !workspace.is_dir() {
        return io::Result::Err(err);
    }

    let path = fs::read_dir(workspace)
    .await?
    .next_entry()
    .await?
    .ok_or(err)?
    .path();

    Ok(NamedFile::open_async(path).await)
}

async fn connect(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Box<dyn Error>> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    let addr = req.peer_addr().unwrap();
    let handle = handle(
        msg_stream,
        session,
        addr
    );

    task::spawn_local(handle);

    Ok(res)
}

async fn uid() -> String {
    // let uid = UID.fetch_add(1, Ordering::SeqCst);

    let uid = {
        let mut rng = thread_rng();
        rng.gen_range(0..u64::MAX)
    };

    let mut hasher = DefaultHasher::new();
    uid.hash(&mut hasher);
    let mut hash = hasher.finish();

    let mut link = String::with_capacity(5);
    for _ in 0..5 {
        let ch = (b'A' + (hash % 26) as u8) as char;
        link.push(ch);
        hash /= 26;
    }

    link
}

async fn handle(
    mut stream: MessageStream,
    mut session: Session,
    addr: SocketAddr,
) {

    let uid = uid().await;
    let workspace = PathBuf::from(format!(".\\workspace\\{}", uid));

    if workspace.exists() {
        let _ = fs::remove_dir_all(&workspace).await;
    }

    let (tx, mut rx) = unbounded_channel::<Message>();

    let mut hb = Instant::now();
    let mut hb_interval = interval(HEARTBEAT_TICK);

    let mut writing = vec![];

    let reason = loop {
        select! {
            _ = hb_interval.tick() => {
                if Instant::now().duration_since(hb) > TIMEOUT || session.ping(b"").await.is_err() {
                    break None;
                }
            }

            Some(Ok(msg)) = stream.recv() => {

                match msg {
                    WsMessage::Text(text) => 'b: {
                        let Ok(message) = json::from_str::<Message>(&text) else { break 'b };
                        match message {
                            Message::File { file, size } => {
                                let _ = fs::create_dir_all(&workspace).await;

                                let file = fs::OpenOptions::new()
                                .write(true)
                                .append(true)
                                .create(true)
                                .open(workspace.join(file))
                                .await
                                .unwrap();

                                writing.push(Writing {
                                    file,
                                    size,
                                    wrote: 0
                                });
                            }
                            _ => {}
                        }
                    }
                    WsMessage::Binary(bytes) => {
                        let Some(w) = writing.get_mut(0) else { break None };

                        w.file.write_all(&bytes).await.unwrap();

                        w.wrote += bytes.len() as u64;

                        if w.wrote == w.size {
                            writing.remove(0);

                            let message = Message::Link {
                                link: format!("{uid}")
                            };
                            let json = json::to_string(&message).unwrap();
                            let _ = session.text(json).await;
                        }
                    }
                    WsMessage::Ping(bytes) => {
                        hb = Instant::now();
                        let _ = session.pong(&bytes).await;
                    }
                    WsMessage::Pong(_bytes) => {
                        hb = Instant::now();
                    }
                    WsMessage::Close(reason) => {
                        break reason;
                    }
                    _ => {}
                }
            }
        
            Some(message) = rx.recv() => {
                let json = json::to_string(&message).unwrap();
                let _ = session.text(json).await;
            }
        }
    };

    let _ = session.close(reason).await;
}

struct Writing {
    file: File,
    size: u64,
    wrote: u64,
}

#[derive(Serialize, Deserialize)]
enum Message {
    File {
        file: String,
        size: u64
    },
    Link {
        link: String
    }
}