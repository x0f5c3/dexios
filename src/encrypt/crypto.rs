use crate::global::enums::{Algorithm, BenchMode, CipherMode, HashMode, OutputFile};
use crate::global::structs::{Header, HeaderType};
use crate::global::{BLOCK_SIZE, VERSION};
use crate::header::sign;
use crate::key::{argon2_hash, gen_salt};
use crate::secret::Secret;
use crate::streams::init_encryption_stream;
use aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use deoxys::DeoxysII256;
use paris::success;
use rand::{prelude::StdRng, Rng, SeedableRng};
use std::fs::File;
use std::io::Read;
use std::result::Result::Ok;
use std::time::Instant;

// this encrypts data in memory mode
// it takes the data and a Secret<> key
// it generates the nonce, hashes the key and encrypts the data
// it writes the header and then the encrypted data to the output file
pub fn encrypt_bytes_memory_mode(
    data: Secret<Vec<u8>>,
    output: &mut OutputFile,
    raw_key: Secret<Vec<u8>>,
    bench: BenchMode,
    hash: HashMode,
    algorithm: Algorithm,
) -> Result<()> {
    let salt = gen_salt();

    let header_type = HeaderType {
        header_version: VERSION,
        cipher_mode: CipherMode::MemoryMode,
        algorithm,
    };

    let (header, signature, encrypted_bytes) = match algorithm {
        Algorithm::Aes256Gcm => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 12]>();
            let nonce = Nonce::from_slice(nonce_bytes.as_slice());

            let header = Header {
                salt,
                nonce: nonce_bytes.to_vec(),
                header_type,
            };
        
            let key = argon2_hash(raw_key, &salt, &header.header_type.header_version)?;

            let cipher = match Aes256Gcm::new_from_slice(key.expose()) {
                Ok(cipher) => {
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let signature = sign(&header, key)?;

            let encrypted_bytes = match cipher.encrypt(nonce, data.expose().as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            drop(data);

            (header, signature, encrypted_bytes)
        }
        Algorithm::XChaCha20Poly1305 => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 24]>();
            let nonce = XNonce::from_slice(&nonce_bytes);

            let header = Header {
                salt,
                nonce: nonce_bytes.to_vec(),
                header_type,
            };

            let key = argon2_hash(raw_key, &salt, &header.header_type.header_version)?;


            let cipher = match XChaCha20Poly1305::new_from_slice(key.expose()) {
                Ok(cipher) => {
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let signature = sign(&header, key)?;

            let encrypted_bytes = match cipher.encrypt(nonce, data.expose().as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            drop(data);

            (header, signature, encrypted_bytes)
        }
        Algorithm::DeoxysII256 => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 15]>();
            let nonce = deoxys::Nonce::from_slice(&nonce_bytes);

            let header = Header {
                salt,
                nonce: nonce_bytes.to_vec(),
                header_type,
            };

            let key = argon2_hash(raw_key, &salt, &header.header_type.header_version)?;

            let cipher = match DeoxysII256::new_from_slice(key.expose()) {
                Ok(cipher) => {
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let signature = sign(&header, key)?;

            let encrypted_bytes = match cipher.encrypt(nonce, data.expose().as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            drop(data);

            (header, signature, encrypted_bytes)
        }
    };

    if bench == BenchMode::WriteToFilesystem {
        let write_start_time = Instant::now();
        crate::header::write_to_file(output, &header, Some(signature.clone()))?;
        output.write_all(&encrypted_bytes)?;
        let write_duration = write_start_time.elapsed();
        success!("Wrote to file [took {:.2}s]", write_duration.as_secs_f32());
    }

    let mut hasher = blake3::Hasher::new();
    if hash == HashMode::CalculateHash {
        let hash_start_time = Instant::now();
        crate::header::hash(&mut hasher, &header, Some(signature));
        hasher.update(&encrypted_bytes);
        let hash = hasher.finalize().to_hex().to_string();
        let hash_duration = hash_start_time.elapsed();
        success!(
            "Hash of the encrypted file is: {} [took {:.2}s]",
            hash,
            hash_duration.as_secs_f32()
        );
    }

    Ok(())
}

// this encrypts data in stream mode
// it takes an input file handle, an output file handle, a Secret<> key, and enums for specific modes
// it gets the nonce, salt and streams enum from `init_encryption_stream` and then reads the file in blocks
// on each read, it encrypts, writes (if enabled), hashes (if enabled) and repeats until EOF
// it also handles the prep of each individual stream, via the match statement
pub fn encrypt_bytes_stream_mode(
    input: &mut File,
    output: &mut OutputFile,
    raw_key: Secret<Vec<u8>>,
    bench: BenchMode,
    hash: HashMode,
    algorithm: Algorithm,
) -> Result<()> {
    let header_type = HeaderType {
        header_version: VERSION,
        cipher_mode: CipherMode::StreamMode,
        algorithm,
    };

    let (mut streams, header, signature) = init_encryption_stream(raw_key, header_type)?;

    if bench == BenchMode::WriteToFilesystem {
        crate::header::write_to_file(output, &header, Some(signature.clone()))?;
    }

    let mut hasher = blake3::Hasher::new();

    if hash == HashMode::CalculateHash {
        crate::header::hash(&mut hasher, &header, Some(signature));
    }

    let mut buffer = [0u8; BLOCK_SIZE];

    loop {
        let read_count = input
            .read(&mut buffer)
            .context("Unable to read from the input file")?;
        if read_count == BLOCK_SIZE {
            let encrypted_data = match streams.encrypt_next(buffer.as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            if bench == BenchMode::WriteToFilesystem {
                output
                    .write_all(&encrypted_data)
                    .context("Unable to write to the output file")?;
            }
            if hash == HashMode::CalculateHash {
                hasher.update(&encrypted_data);
            }
        } else {
            // if we read something less than BLOCK_SIZE, and have hit the end of the file
            let encrypted_data = match streams.encrypt_last(&buffer[..read_count]) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            if bench == BenchMode::WriteToFilesystem {
                output
                    .write_all(&encrypted_data)
                    .context("Unable to write to the output file")?;
            }
            if hash == HashMode::CalculateHash {
                hasher.update(&encrypted_data);
            }
            break;
        }
    }
    if bench == BenchMode::WriteToFilesystem {
        output.flush().context("Unable to flush the output file")?;
    }
    if hash == HashMode::CalculateHash {
        let hash = hasher.finalize().to_hex().to_string();
        success!("Hash of the encrypted file is: {}", hash,);
    }
    Ok(())
}
