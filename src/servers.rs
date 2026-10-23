//! Binary NBT encoder for Minecraft servers.dat

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerEntry {
    pub name: String,
    pub ip: String,
    #[serde(default = "default_accept_textures")]
    pub accept_textures: bool,
}

fn default_accept_textures() -> bool {
    true
}

/// Encodes a list of server entries into standard Minecraft servers.dat (NBT compound format).
pub fn encode_servers_dat(servers: &[ServerEntry]) -> Vec<u8> {
    let mut out = Vec::new();

    // TAG_Compound (10) - Root tag
    out.push(10);
    // Compound name: "" (length 0)
    out.extend_from_slice(&0u16.to_be_bytes());

    // TAG_List (9)
    out.push(9);
    // List name: "servers"
    let list_name = b"servers";
    out.extend_from_slice(&(list_name.len() as u16).to_be_bytes());
    out.extend_from_slice(list_name);

    // List elements type: TAG_Compound (10)
    out.push(10);
    // Number of entries: i32
    out.extend_from_slice(&(servers.len() as i32).to_be_bytes());

    for s in servers {
        // Tag String (8), name = "name"
        out.push(8);
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(b"name");
        let name_bytes = s.name.as_bytes();
        out.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(name_bytes);

        // Tag String (8), name = "ip"
        out.push(8);
        out.extend_from_slice(&2u16.to_be_bytes());
        out.extend_from_slice(b"ip");
        let ip_bytes = s.ip.as_bytes();
        out.extend_from_slice(&(ip_bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(ip_bytes);

        // Tag Byte (1), name = "acceptTextures"
        out.push(1);
        out.extend_from_slice(&14u16.to_be_bytes());
        out.extend_from_slice(b"acceptTextures");
        out.push(if s.accept_textures { 1 } else { 0 });

        // Tag End (0) - end of server compound
        out.push(0);
    }

    // Tag End (0) - end of root compound
    out.push(0);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_servers_dat() {
        let servers = vec![
            ServerEntry {
                name: "Main Server".to_string(),
                ip: "100.127.101.226:25565".to_string(),
                accept_textures: true,
            },
        ];
        let bytes = encode_servers_dat(&servers);
        assert_eq!(bytes[0], 10); // TAG_Compound
        assert_eq!(&bytes[1..3], &[0, 0]); // empty name
        assert_eq!(bytes[3], 9); // TAG_List
        assert_eq!(*bytes.last().unwrap(), 0); // TAG_End
    }
}
