//! The Click / Play session: ECDH P-256 key agreement, HKDF-SHA256 key derivation and
//! AES-256-CCM framing (4-byte tag, 8-byte nonce).
//!
//! Handshake (protocol facts from `virtual-cog` and Makinolo's notes):
//!
//! * the app writes `"RideOn"` + `[0x01, 0x02]` + its 64-byte public key (X‖Y, no `0x04`);
//! * the device answers `"RideOn"` + two header bytes + its own 64-byte public key;
//! * a shorter answer means the device runs **without** encryption (plain packets);
//! * otherwise `shared = X coordinate of ECDH`, and
//!   `HKDF-SHA256(ikm = shared, salt = device_pub ‖ app_pub, info = "", len = 36)` yields the
//!   AES key (first 32 bytes) and the nonce suffix (last 4 bytes).
//!
//! Packets: `counter (4 bytes LE) ‖ ciphertext ‖ MIC (4 bytes)`; the nonce is
//! `nonce_suffix (4) ‖ counter (4)`, there is no associated data, and the transmit counter
//! of the device starts at 0. To avoid bidirectional nonce reuse while preserving the
//! device's shared-key protocol, app transmission uses the upper half of the counter space.
//! Receive counters in that half are refused and transmission never wraps. This assumes the
//! controller follows its incremental counter protocol; compatibility needs a hardware check.
//!
//! The key material never leaves this module and is not printed by `Debug`.

use aes::Aes256;
use ccm::aead::{Aead, KeyInit, Payload};
use ccm::consts::{U4, U8};
use ccm::Ccm;
use hkdf::Hkdf;
use p256::ecdh::EphemeralSecret;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::elliptic_curve::zeroize::Zeroize;
use p256::{EncodedPoint, PublicKey};
use rand_core::OsRng;
use sha2::Sha256;

use super::ZwiftError;

/// Greeting that starts the handshake in both directions.
pub const RIDE_ON: &[u8] = b"RideOn";
/// Protocol version bytes the app sends after `RideOn`.
const HELLO_VERSION: [u8; 2] = [0x01, 0x02];
/// Length of an uncompressed P-256 public key without its `0x04` prefix.
const PUBLIC_KEY_LEN: usize = 64;
/// `RideOn` + 2 header bytes + device public key.
const DEVICE_HELLO_LEN: usize = RIDE_ON.len() + 2 + PUBLIC_KEY_LEN;
/// Offset of the device public key inside the device hello.
const DEVICE_KEY_OFFSET: usize = RIDE_ON.len() + 2;
/// Bytes of the packet counter (little endian) in front of every encrypted packet.
const COUNTER_LEN: usize = 4;
/// CCM authentication tag length.
const TAG_LEN: usize = 4;
/// Smallest encrypted packet: counter + tag (an empty plaintext).
const MIN_PACKET_LEN: usize = COUNTER_LEN + TAG_LEN;
/// Derived key material: 32-byte AES key followed by a 4-byte nonce suffix.
const KEY_MATERIAL_LEN: usize = 36;
const APP_COUNTER_START: u32 = 1 << 31;

/// AES-256-CCM with a 4-byte tag and an 8-byte nonce (as used by the controllers).
type Cipher = Ccm<Aes256, U4, U8>;

/// Whether the device encrypts its packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encryption {
    Encrypted,
    Plain,
}

/// Derived session keys. Deliberately not `Debug`.
#[derive(Clone)]
pub struct SessionKeys {
    key: [u8; 32],
    nonce_suffix: [u8; 4],
}

impl Drop for SessionKeys {
    fn drop(&mut self) {
        self.key.zeroize();
        self.nonce_suffix.zeroize();
    }
}

impl SessionKeys {
    /// Keys from raw material (tests and the in-test device use this directly).
    pub fn new(key: [u8; 32], nonce_suffix: [u8; 4]) -> Self {
        Self { key, nonce_suffix }
    }
}

/// Where the handshake stands.
enum Session {
    /// No device answer yet: nothing can be sent or read.
    Handshake,
    Encrypted(SessionKeys),
    Plain,
}

/// The app side of one controller connection.
pub struct ClickCrypto {
    secret: Option<EphemeralSecret>,
    app_public: [u8; PUBLIC_KEY_LEN],
    session: Session,
    tx_counter: u32,
    /// Counter of the last device packet that opened: a packet must carry a larger one, so a
    /// recorded button press cannot be played back.
    rx_counter: Option<u32>,
}

impl Default for ClickCrypto {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickCrypto {
    pub fn new() -> Self {
        Self {
            secret: None,
            app_public: [0; PUBLIC_KEY_LEN],
            session: Session::Handshake,
            tx_counter: APP_COUNTER_START,
            rx_counter: None,
        }
    }

    /// Generates a fresh key pair (the handshake restarts) and returns the hello to send.
    pub fn hello_payload(&mut self) -> Vec<u8> {
        let secret = EphemeralSecret::random(&mut OsRng);
        // the 65-byte uncompressed form; the first byte is the 0x04 marker we drop
        let point: EncodedPoint = secret.public_key().to_encoded_point(false);
        let bytes = point.as_bytes();
        if bytes.len() == PUBLIC_KEY_LEN + 1 {
            self.app_public.copy_from_slice(&bytes[1..]);
        }
        self.secret = Some(secret);
        self.session = Session::Handshake;
        self.tx_counter = APP_COUNTER_START;
        self.rx_counter = None;

        let mut out = Vec::with_capacity(RIDE_ON.len() + HELLO_VERSION.len() + PUBLIC_KEY_LEN);
        out.extend_from_slice(RIDE_ON);
        out.extend_from_slice(&HELLO_VERSION);
        out.extend_from_slice(&self.app_public);
        out
    }

    /// Reads the device's answer to our hello. Only during the handshake: a second answer
    /// must neither downgrade an encrypted session to plain nor restart the transmit counter
    /// under the same key (a CCM nonce used twice).
    pub fn on_device_hello(&mut self, bytes: &[u8]) -> Result<Encryption, ZwiftError> {
        if !bytes.starts_with(RIDE_ON) || !matches!(self.session, Session::Handshake) {
            return Err(ZwiftError::BadHandshake);
        }
        if bytes.len() < DEVICE_HELLO_LEN {
            // "RideOn" without a key: the device does not encrypt.
            self.session = Session::Plain;
            return Ok(Encryption::Plain);
        }
        let device_public = &bytes[DEVICE_KEY_OFFSET..DEVICE_KEY_OFFSET + PUBLIC_KEY_LEN];
        let secret = self.secret.as_ref().ok_or(ZwiftError::MissingKey)?;

        let mut sec1 = [0u8; PUBLIC_KEY_LEN + 1];
        sec1[0] = 0x04;
        sec1[1..].copy_from_slice(device_public);
        let peer = PublicKey::from_sec1_bytes(&sec1).map_err(|_| ZwiftError::BadHandshake)?;
        let shared = secret.diffie_hellman(&peer);
        let keys = derive_keys(shared.raw_secret_bytes(), device_public, &self.app_public)?;

        self.secret = None; // EphemeralSecret zeroizes its private scalar on drop.
        self.session = Session::Encrypted(keys);
        self.tx_counter = APP_COUNTER_START;
        Ok(Encryption::Encrypted)
    }

    /// `true` once the device answered (encrypted or plain).
    pub fn is_ready(&self) -> bool {
        !matches!(self.session, Session::Handshake)
    }

    /// The negotiated mode, once known.
    pub fn encryption(&self) -> Option<Encryption> {
        match self.session {
            Session::Handshake => None,
            Session::Encrypted(_) => Some(Encryption::Encrypted),
            Session::Plain => Some(Encryption::Plain),
        }
    }

    /// Turns a device packet into its plaintext (`opcode ‖ payload`). An encrypted packet whose
    /// counter is not past the last one that opened is a replay and fails like a forged one.
    pub fn decrypt(&mut self, packet: &[u8]) -> Result<Vec<u8>, ZwiftError> {
        match &self.session {
            Session::Handshake => Err(ZwiftError::NotReady),
            Session::Plain => Ok(packet.to_vec()),
            Session::Encrypted(keys) => {
                let plain = open(keys, packet)?;
                let counter = packet_counter(packet).ok_or(ZwiftError::ShortPacket)?;
                if counter >= APP_COUNTER_START
                    || self.rx_counter.is_some_and(|last| counter <= last)
                {
                    return Err(ZwiftError::AuthFailed);
                }
                self.rx_counter = Some(counter);
                Ok(plain)
            }
        }
    }

    /// Seals a plaintext packet with the next transmit counter.
    pub fn encrypt(&mut self, plain: &[u8]) -> Result<Vec<u8>, ZwiftError> {
        match &self.session {
            Session::Handshake => Err(ZwiftError::NotReady),
            Session::Plain => Ok(plain.to_vec()),
            Session::Encrypted(keys) => {
                let counter = self.tx_counter;
                self.tx_counter = self.tx_counter.checked_add(1).ok_or(ZwiftError::Crypto)?;
                seal(keys, counter, plain)
            }
        }
    }
}

/// HKDF-SHA256 of the ECDH secret, salted with both public keys (device first).
pub fn derive_keys(
    shared: &[u8],
    device_public: &[u8],
    app_public: &[u8],
) -> Result<SessionKeys, ZwiftError> {
    let mut salt = Vec::with_capacity(device_public.len() + app_public.len());
    salt.extend_from_slice(device_public);
    salt.extend_from_slice(app_public);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut okm = [0u8; KEY_MATERIAL_LEN];
    hk.expand(&[], &mut okm).map_err(|_| ZwiftError::Crypto)?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&okm[..32]);
    let mut nonce_suffix = [0u8; 4];
    nonce_suffix.copy_from_slice(&okm[32..]);
    let keys = SessionKeys { key, nonce_suffix };
    okm.zeroize();
    key.zeroize();
    nonce_suffix.zeroize();
    Ok(keys)
}

fn cipher(keys: &SessionKeys) -> Result<Cipher, ZwiftError> {
    Cipher::new_from_slice(&keys.key).map_err(|_| ZwiftError::Crypto)
}

fn nonce(keys: &SessionKeys, counter_le: &[u8; 4]) -> [u8; 8] {
    let mut n = [0u8; 8];
    n[..4].copy_from_slice(&keys.nonce_suffix);
    n[4..].copy_from_slice(counter_le);
    n
}

/// `counter ‖ AES-CCM(plain) ‖ tag` for the given counter.
pub fn seal(keys: &SessionKeys, counter: u32, plain: &[u8]) -> Result<Vec<u8>, ZwiftError> {
    let counter_le = counter.to_le_bytes();
    let n = nonce(keys, &counter_le);
    let sealed = cipher(keys)?
        .encrypt(
            (&n).into(),
            Payload {
                msg: plain,
                aad: &[],
            },
        )
        .map_err(|_| ZwiftError::Crypto)?;
    let mut out = Vec::with_capacity(COUNTER_LEN + sealed.len());
    out.extend_from_slice(&counter_le);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// The little-endian counter in front of a sealed packet.
fn packet_counter(packet: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = packet.get(..COUNTER_LEN)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// Inverse of [`seal`]. Fails for short packets and for any authentication mismatch.
pub fn open(keys: &SessionKeys, packet: &[u8]) -> Result<Vec<u8>, ZwiftError> {
    if packet.len() < MIN_PACKET_LEN {
        return Err(ZwiftError::ShortPacket);
    }
    let (counter, body) = packet.split_at(COUNTER_LEN);
    let mut counter_le = [0u8; 4];
    counter_le.copy_from_slice(counter);
    let n = nonce(keys, &counter_le);
    cipher(keys)?
        .decrypt(
            (&n).into(),
            Payload {
                msg: body,
                aad: &[],
            },
        )
        .map_err(|_| ZwiftError::AuthFailed)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Simulates the device: its own key pair and the same derivation as the app.
    pub(crate) struct Device {
        secret: EphemeralSecret,
        public: [u8; 64],
    }

    impl Device {
        pub(crate) fn new() -> Self {
            let secret = EphemeralSecret::random(&mut OsRng);
            let point: EncodedPoint = secret.public_key().to_encoded_point(false);
            let mut public = [0u8; 64];
            public.copy_from_slice(&point.as_bytes()[1..]);
            Self { secret, public }
        }

        /// The device's answer to the app's hello.
        pub(crate) fn hello(&self) -> Vec<u8> {
            let mut out = RIDE_ON.to_vec();
            out.extend_from_slice(&[0x00, 0x09]);
            out.extend_from_slice(&self.public);
            out
        }

        /// The keys the device derives from the app's hello.
        pub(crate) fn keys(&self, app_hello: &[u8]) -> SessionKeys {
            let app_public = &app_hello[RIDE_ON.len() + 2..];
            let mut sec1 = vec![0x04];
            sec1.extend_from_slice(app_public);
            let peer = PublicKey::from_sec1_bytes(&sec1).unwrap();
            let shared = self.secret.diffie_hellman(&peer);
            derive_keys(shared.raw_secret_bytes(), &self.public, app_public).unwrap()
        }
    }

    fn handshaken() -> (ClickCrypto, SessionKeys) {
        let device = Device::new();
        let mut app = ClickCrypto::new();
        let hello = app.hello_payload();
        assert_eq!(hello.len(), 72);
        assert_eq!(&hello[..6], b"RideOn");
        assert_eq!(&hello[6..8], &[0x01, 0x02]);
        assert_eq!(
            app.on_device_hello(&device.hello()).unwrap(),
            Encryption::Encrypted
        );
        (app, device.keys(&hello))
    }

    #[test]
    fn full_handshake_between_two_parties_agrees_on_keys() {
        let device = Device::new();
        let mut app = ClickCrypto::new();
        let hello = app.hello_payload();
        assert!(!app.is_ready());
        assert_eq!(app.encryption(), None);
        assert_eq!(
            app.on_device_hello(&device.hello()).unwrap(),
            Encryption::Encrypted
        );
        assert!(app.is_ready());
        assert_eq!(app.encryption(), Some(Encryption::Encrypted));

        // what the app encrypts, the device opens with its own derivation
        let keys = device.keys(&hello);
        let packet = app.encrypt(&[0x37, 0x08, 0x00]).unwrap();
        assert_eq!(open(&keys, &packet).unwrap(), vec![0x37, 0x08, 0x00]);
        // and the other way round
        let packet = seal(&keys, 7, &[0x15]).unwrap();
        assert_eq!(app.decrypt(&packet).unwrap(), vec![0x15]);
        // the same packet again is a replay, an older counter too; a newer one opens
        assert_eq!(app.decrypt(&packet), Err(ZwiftError::AuthFailed));
        let older = seal(&keys, 3, &[0x15]).unwrap();
        assert_eq!(app.decrypt(&older), Err(ZwiftError::AuthFailed));
        let newer = seal(&keys, 8, &[0x16]).unwrap();
        assert_eq!(app.decrypt(&newer).unwrap(), vec![0x16]);
    }

    #[test]
    fn a_second_device_hello_neither_downgrades_nor_resets_the_counter() {
        let (mut app, keys) = handshaken();
        let first = app.encrypt(b"a").unwrap();
        assert_eq!(
            app.on_device_hello(b"RideOn"),
            Err(ZwiftError::BadHandshake)
        );
        assert_eq!(app.encryption(), Some(Encryption::Encrypted));
        let second = app.encrypt(b"a").unwrap();
        assert_ne!(&first[..4], &second[..4], "the counter went on");
        assert_eq!(open(&keys, &second).unwrap(), b"a");
    }

    #[test]
    fn encrypt_decrypt_round_trip_and_counter_advances() {
        let (mut app, keys) = handshaken();
        let first = app.encrypt(b"hello").unwrap();
        let second = app.encrypt(b"hello").unwrap();
        assert_eq!(&first[..4], &APP_COUNTER_START.to_le_bytes());
        assert_eq!(&second[..4], &(APP_COUNTER_START + 1).to_le_bytes());
        assert_ne!(first, second, "a fresh nonce for every packet");
        assert_eq!(open(&keys, &first).unwrap(), b"hello");
        assert_eq!(open(&keys, &second).unwrap(), b"hello");
        // empty plaintext is still a valid packet (counter + tag)
        let empty = app.encrypt(&[]).unwrap();
        assert_eq!(empty.len(), MIN_PACKET_LEN);
        assert_eq!(open(&keys, &empty).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn directions_use_disjoint_counters_and_transmission_never_wraps() {
        let (mut app, keys) = handshaken();
        let tx = app.encrypt(b"haptic").unwrap();
        let rx = seal(&keys, 0, b"button").unwrap();
        assert_ne!(&tx[..4], &rx[..4]);
        assert_eq!(app.decrypt(&rx).unwrap(), b"button");
        let reflected = app.encrypt(b"haptic").unwrap();
        assert_eq!(app.decrypt(&reflected), Err(ZwiftError::AuthFailed));
        app.tx_counter = u32::MAX;
        assert_eq!(app.encrypt(b"x"), Err(ZwiftError::Crypto));
        assert_eq!(app.encrypt(b"x"), Err(ZwiftError::Crypto));
    }

    #[test]
    fn tampered_mic_fails() {
        let (mut app, keys) = handshaken();
        let mut packet = app.encrypt(&[0x37, 0x08, 0x00]).unwrap();
        *packet.last_mut().unwrap() ^= 0x01;
        assert_eq!(open(&keys, &packet), Err(ZwiftError::AuthFailed));
        let mut packet = app.encrypt(&[0x37, 0x08, 0x00]).unwrap();
        packet[4] ^= 0x80;
        assert_eq!(open(&keys, &packet), Err(ZwiftError::AuthFailed));
        // the counter is authenticated too: a changed counter changes the nonce
        let mut packet = app.encrypt(&[0x37]).unwrap();
        packet[0] ^= 0x01;
        assert_eq!(open(&keys, &packet), Err(ZwiftError::AuthFailed));
    }

    #[test]
    fn short_packets_fail_without_panicking() {
        let (mut app, keys) = handshaken();
        for len in 0..MIN_PACKET_LEN {
            let packet = vec![0xAA; len];
            assert_eq!(open(&keys, &packet), Err(ZwiftError::ShortPacket));
            assert!(app.decrypt(&packet).is_err());
        }
    }

    #[test]
    fn short_device_answer_means_plain_packets() {
        let mut app = ClickCrypto::new();
        app.hello_payload();
        assert_eq!(app.on_device_hello(b"RideOn").unwrap(), Encryption::Plain);
        assert_eq!(app.encryption(), Some(Encryption::Plain));
        // plain sessions pass packets through unchanged
        assert_eq!(app.encrypt(&[0x37, 0x08]).unwrap(), vec![0x37, 0x08]);
        assert_eq!(app.decrypt(&[0x37, 0x08]).unwrap(), vec![0x37, 0x08]);
        // a 16-byte answer is shorter than a key too
        let mut app = ClickCrypto::new();
        app.hello_payload();
        assert_eq!(
            app.on_device_hello(&[
                b'R', b'i', b'd', b'e', b'O', b'n', 0, 9, 1, 2, 3, 4, 5, 6, 7, 8
            ])
            .unwrap(),
            Encryption::Plain
        );
    }

    #[test]
    fn bad_handshakes_are_errors() {
        let mut app = ClickCrypto::new();
        assert_eq!(
            app.on_device_hello(b"RideOn"),
            Ok(Encryption::Plain),
            "a plain answer needs no local key"
        );
        let mut app = ClickCrypto::new();
        assert_eq!(app.on_device_hello(b"Nope"), Err(ZwiftError::BadHandshake));
        // a full-length answer without our key is a missing key
        let mut app = ClickCrypto::new();
        let mut long = RIDE_ON.to_vec();
        long.extend_from_slice(&[0; 66]);
        assert_eq!(app.on_device_hello(&long), Err(ZwiftError::MissingKey));
        // a point that is not on the curve is rejected
        let mut app = ClickCrypto::new();
        app.hello_payload();
        let mut bogus = RIDE_ON.to_vec();
        bogus.extend_from_slice(&[0, 9]);
        bogus.extend_from_slice(&[0x11; 64]);
        assert_eq!(app.on_device_hello(&bogus), Err(ZwiftError::BadHandshake));
    }

    #[test]
    fn nothing_flows_before_the_handshake() {
        let mut app = ClickCrypto::new();
        assert_eq!(app.encrypt(&[1]), Err(ZwiftError::NotReady));
        assert_eq!(app.decrypt(&[0; 8]), Err(ZwiftError::NotReady));
    }

    /// Known-answer tests from NIST SP 800-38C (AES-128 key `40 41 .. 4f`), taken from the
    /// `ccm` crate's own test suite. The first has a 7-byte nonce and a 4-byte tag; the
    /// second has the 8-byte nonce this protocol uses (with a 6-byte tag). They check the
    /// CCM construction the controller cipher is built on; only the key size differs.
    #[test]
    fn nist_sp800_38c_known_answers() {
        use aes::Aes128;
        let key = hex("404142434445464748494a4b4c4d4e4f");

        // (tag 4, nonce 7): nonce 10111213141516, aad 00..07, payload 20212223
        check_vector::<Ccm<Aes128, U4, ccm::consts::U7>>(
            &key,
            &hex("10111213141516"),
            &hex("0001020304050607"),
            &hex("20212223"),
            &hex("7162015b4dac255d"),
        );
        // (tag 6, nonce 8): nonce 1011121314151617, aad 00..0f, payload 20..2f
        check_vector::<Ccm<Aes128, ccm::consts::U6, U8>>(
            &key,
            &hex("1011121314151617"),
            &hex("000102030405060708090a0b0c0d0e0f"),
            &hex("202122232425262728292a2b2c2d2e2f"),
            &hex("d2a1f0e051ea5f62081a7792073d593d1fc64fbfaccd"),
        );
    }

    fn check_vector<C>(key: &[u8], nonce: &[u8], aad: &[u8], payload: &[u8], expected: &[u8])
    where
        C: KeyInit + Aead,
    {
        let cipher = C::new_from_slice(key).unwrap();
        let sealed = cipher
            .encrypt(nonce.into(), Payload { msg: payload, aad })
            .unwrap();
        assert_eq!(sealed, expected);
        let opened = cipher
            .decrypt(nonce.into(), Payload { msg: expected, aad })
            .unwrap();
        assert_eq!(opened, payload);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("test hex"))
            .collect()
    }
}
