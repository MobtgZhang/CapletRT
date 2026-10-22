//! End-to-end A/B update and secure-boot scenarios over one RAM flash image.

use caplet_ota::{
    BootManager, BootOutcome, BootSlot, BootState, Bootloader, DownloadError, FirmwareHeader,
    FlashStorage, RamFlash, TrustStore, Updater, Verifier,
};
use caplet_security::flash::{FlashLayout, FlashRegion};
use ed25519_dalek::{Signer, SigningKey};

const FLASH_SIZE: usize = 256 * 1024;
const KEY_ID: u32 = 0x4f_54_41;

fn layout() -> FlashLayout {
    FlashLayout::new(FLASH_SIZE as u32, 4096, 32 * 1024)
}

fn trust() -> (TrustStore, SigningKey) {
    let key = SigningKey::from_bytes(&[0x42; 32]);
    let mut store = TrustStore::new();
    assert!(store.add(KEY_ID, key.verifying_key().to_bytes()));
    (store, key)
}

fn sign(image: &[u8], version: u32, key: &SigningKey) -> [u8; 64] {
    let header = FirmwareHeader::new(version, image, KEY_ID);
    let mut message = Vec::with_capacity(64 + image.len());
    message.extend_from_slice(&header.to_bytes());
    message.extend_from_slice(image);
    key.sign(&message).to_bytes()
}

fn region_len(region: FlashRegion) -> u32 {
    region.end - region.start + 1
}

fn provision_slot(
    flash: &mut RamFlash<FLASH_SIZE>,
    region: FlashRegion,
    image: &[u8],
    version: u32,
    signature: &[u8; 64],
) {
    let header = FirmwareHeader::new(version, image, KEY_ID);
    flash.unlock().unwrap();
    flash.erase(region.start, region_len(region)).unwrap();
    flash.program(region.start, &header.to_bytes()).unwrap();
    flash.program(region.start + 64, image).unwrap();
    flash.program(region.end + 1 - 64, signature).unwrap();
    flash.lock().unwrap();
}

#[test]
fn signed_update_becomes_a_verified_trial_boot() {
    let (store, key) = trust();
    let layout = layout();
    let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);

    let primary = b"factory firmware";
    let primary_sig = sign(primary, 1, &key);
    provision_slot(&mut flash, layout.primary, primary, 1, &primary_sig);

    let update = b"field update payload";
    let update_sig = sign(update, 2, &key);
    let mut updater = Updater::new(layout, &mut flash, store.clone());
    let result = updater
        .install_signed(update, 2, KEY_ID, &update_sig)
        .expect("signed update installs");
    assert_eq!(result.slot, BootSlot::Secondary);
    assert_eq!(result.header.version, 2);

    let mut manager = BootManager::new(layout, &mut flash);
    assert_eq!(manager.select_slot().unwrap(), BootSlot::Secondary);
    assert_eq!(manager.boot_state().unwrap(), BootState::TryingUpdate);
    manager.mark_update_success(BootSlot::Secondary, 2).unwrap();
    assert_eq!(manager.boot_state().unwrap(), BootState::Normal);

    let mut bootloader = Bootloader::new(layout, flash, store);
    match bootloader.boot() {
        BootOutcome::Boot(selection) => {
            assert_eq!(selection.slot, BootSlot::Secondary);
            assert_eq!(selection.header.version, 2);
            assert!(!selection.trial, "confirmed update must not stay in trial");
            assert!(!selection.rolled_back);
        }
        BootOutcome::Halt(error) => panic!("secure boot halted: {error:?}"),
    }
}

#[test]
fn rejected_update_leaves_primary_and_boot_records_untouched() {
    let (store, key) = trust();
    let layout = layout();
    let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);

    let primary = b"stable primary";
    let primary_sig = sign(primary, 1, &key);
    provision_slot(&mut flash, layout.primary, primary, 1, &primary_sig);
    let before = flash.as_slice().to_vec();

    let update = b"rejected update";
    let mut bad_sig = sign(update, 2, &key);
    bad_sig[7] ^= 0x80;
    let mut updater = Updater::new(layout, &mut flash, store);
    assert!(matches!(
        updater.install_signed(update, 2, KEY_ID, &bad_sig),
        Err(DownloadError::Verification(_))
    ));

    assert_eq!(flash.as_slice(), before);
    assert_eq!(
        BootManager::new(layout, &mut flash).select_slot().unwrap(),
        BootSlot::Primary
    );
}

#[test]
fn damaged_trial_image_rolls_back_to_authenticated_primary() {
    let (store, key) = trust();
    let layout = layout();
    let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);

    let primary = b"known good primary";
    let primary_sig = sign(primary, 1, &key);
    provision_slot(&mut flash, layout.primary, primary, 1, &primary_sig);

    let update = b"trial image that will be damaged";
    let update_sig = sign(update, 2, &key);
    Updater::new(layout, &mut flash, store.clone())
        .install_signed(update, 2, KEY_ID, &update_sig)
        .expect("trial installs");

    let image_addr = layout.secondary.start as usize + 64;
    flash.as_mut_slice()[image_addr] ^= 0x01;
    assert!(matches!(
        Verifier::new().verify_flash(&mut flash, layout.secondary, &store),
        Err(caplet_ota::VerifyError::HashMismatch)
    ));

    let mut bootloader = Bootloader::new(layout, flash, store);
    match bootloader.boot() {
        BootOutcome::Boot(selection) => {
            assert_eq!(selection.slot, BootSlot::Primary);
            assert_eq!(selection.header.version, 1);
            assert!(selection.rolled_back);
            assert!(!selection.trial);
        }
        BootOutcome::Halt(error) => panic!("rollback should reach primary: {error:?}"),
    }
}

#[test]
fn both_unauthenticated_slots_halt_instead_of_booting_unknown_code() {
    let (store, key) = trust();
    let layout = layout();
    let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);

    let primary = b"primary with broken signature";
    let primary_sig = sign(primary, 1, &key);
    provision_slot(&mut flash, layout.primary, primary, 1, &primary_sig);
    flash.as_mut_slice()[layout.primary.start as usize + 3] ^= 1;

    let mut bootloader = Bootloader::new(layout, flash, store);
    assert!(matches!(bootloader.boot(), BootOutcome::Halt(_)));
}
