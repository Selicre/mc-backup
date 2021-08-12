use tokio::net::TcpStream;
use tokio::io::{self,AsyncRead,AsyncWrite,AsyncWriteExt,AsyncReadExt};
use bytes::{Bytes,BytesMut,Buf,BufMut};

#[derive(Debug)]
pub struct Packet {
    req_id: i32,
    id: i32,
    payload: Bytes
}
impl Packet {
    pub fn read(buf: &mut impl Buf) -> Option<Self> {
        if buf.remaining() < 4 { return None; }
        let len = buf.get_i32_le() as usize;
        if buf.remaining() < len { return None; }
        let req_id = buf.get_i32_le();
        let id = buf.get_i32_le();
        let payload = buf.copy_to_bytes(len-10);
        buf.get_u16();
        Some(Self {
            req_id, id, payload
        })
    }
    pub fn write(&self, buf: &mut impl BufMut) {
        buf.put_i32_le(self.payload.len() as i32 + 10);
        buf.put_i32_le(self.req_id);
        buf.put_i32_le(self.id);
        buf.put(self.payload.chunk());
        buf.put_u16(0);
    }
}

pub struct Connection<C> {
    read_buf: BytesMut,
    write_buf: BytesMut,
    conn: C,
    req_id: i32,
}

impl<C: AsyncRead + AsyncWrite + Unpin> Connection<C> {
    pub fn new(conn: C) -> Self {
        Self {
            conn,
            read_buf: BytesMut::new(),
            write_buf: BytesMut::new(),
            req_id: 0
        }
    }
    pub async fn read(&mut self) -> io::Result<Packet> {
        loop {
            // try to read from the buffer
            let mut b = self.read_buf.chunk();
            let c = Packet::read(&mut b);
            if let Some(c) = c {
                let len = b.as_ptr() as usize - self.read_buf.chunk().as_ptr() as usize;
                self.read_buf.advance(len);
                return Ok(c)
            } else {
                match self.conn.read_buf(&mut self.read_buf).await {
                    Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                    Ok(_) => {
                        //println!("read bytes: {:?}", self.read_buf.chunk())
                    },
                    Err(e) => return Err(e)
                }
            }
        }
    }
    pub fn write(&mut self, id: i32, payload: impl Into<Bytes>) {
        Packet {
            req_id: self.req_id, id, payload: payload.into()
        }.write(&mut self.write_buf);
        self.req_id += 1;
    }
    pub async fn flush(&mut self) -> io::Result<()> {
        self.conn.write_all(&self.write_buf).await?;
        self.write_buf.clear();
        Ok(())
    }
    pub async fn login(&mut self, data: impl Into<Bytes>) -> io::Result<bool> {
        self.write(3, data);
        self.flush().await?;
        let packet = self.read().await?;
        Ok(packet.req_id != -1)
    }
    pub async fn send(&mut self, data: impl Into<Bytes>) -> io::Result<String> {
        self.write(2, data);
        self.flush().await?;
        let packet = self.read().await?;
        Ok(String::from_utf8(packet.payload.to_vec()).unwrap())
    }
}


#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let conn = TcpStream::connect("127.0.0.1:25575").await?;
    let mut c = Connection::new(conn);

    let r = c.login("testpwd").await?;
    println!("{}", r);

    let r = vec![
        c.send("save-off").await?,
        c.send("save-all").await?,
    ];
    println!("{:?}", r);

    let incr_path = &chrono::offset::Local::today().format("backups/index.%Y-%W.snar").to_string();
    let archive_path = &chrono::offset::Local::today().format("backups/backup.%Y-%W.%F.tar").to_string();
    let folder_path = "survival";

    let out = tokio::process::Command::new("tar")
        .args(&[
            "-cg", incr_path, "-f", archive_path, folder_path
        ])
        .output().await?;
    let r = vec![
        c.send("say World backup created!").await?,
        c.send("save-on").await?,
    ];
    println!("{:?}", r);
    Ok(())
}
