//! Güvenlik regresyon testleri: güvenilmeyen dosyadan gelen boyut alanları.
//!
//! Elle üretilmiş tek bir `.sqlite` dosyası SQLStage'i düşürmemelidir. Buradaki
//! fikstürler b-tree sayfa başlıklarına `i64::MAX` gibi sınırsız yük boyutları yazar
//! ve aracın bunları **kontrollü bir `SahneHata`** ile reddettiğini doğrular.
//!
//! `tempfile` bağımlılık politikasıyla yasak olduğu için geçici dosya yardımcısı
//! `tests/yardimci` içindeki `GeciciDizin` ile sağlanır.

mod yardimci;

use std::path::Path;

use sqlstage::deger::Deger;
use sqlstage::hata::SahneHata;
use sqlstage::kayit::kayit_yaz;
use sqlstage::sayfa::{yuk_bol, SayfaTuru};
use sqlstage::varint::varint_yaz;
use sqlstage::veritabani::Veritabani;

use yardimci::GeciciDizin;

/// Fikstürlerde kullanılan sayfa boyutu (küçük tutulur ki varışma zinciri ucuz olsun).
const SAYFA_BOYUTU: usize = 512;

/// `sqlite_master` sayfasını içeren ilk sayfanın b-tree başlık ofseti.
const MASTER_BASLIK_OFSETI: usize = 100;

/// Zehirli tablonun kök sayfa numarası (1. sayfa `sqlite_master`'dır).
const KOK_SAYFA: u32 = 2;

/// Zehirli hücrenin işaret ettiği ilk varışma sayfası.
const VARISMA_SAYFASI: u32 = 3;

/// Geçerli bir 100 baytlık SQLite başlığıyla başlayan, toplam `SAYFA_BOYUTU` baytlık
/// 1. sayfayı üretir. Kalan baytlar sıfırdır (b-tree gövdesi sonra yazılır).
fn birinci_sayfa_taslak(sayfa_sayisi: u32) -> Vec<u8> {
    let mut sayfa = vec![0u8; SAYFA_BOYUTU];
    sayfa[0..16].copy_from_slice(b"SQLite format 3\0");
    sayfa[16..18].copy_from_slice(&(SAYFA_BOYUTU as u16).to_be_bytes());
    sayfa[18] = 1; // dosya yazma sürümü: journal
    sayfa[19] = 1; // dosya okuma sürümü: journal
    sayfa[20] = 0; // rezerve alan
    sayfa[28..32].copy_from_slice(&sayfa_sayisi.to_be_bytes());
    sayfa[44..48].copy_from_slice(&1u32.to_be_bytes()); // şema biçimi 1
    sayfa[56..60].copy_from_slice(&1u32.to_be_bytes()); // metin kodlaması: UTF-8
    sayfa
}

/// Tek hücrelik bir tablo b-tree yaprak sayfası üretir.
///
/// `ofset`, b-tree başlığının sayfa içindeki başlangıç konumudur (1. sayfa için 100).
fn yaprak_sayfa(hucre: &[u8], ofset: usize) -> Vec<u8> {
    let mut sayfa = vec![0u8; SAYFA_BOYUTU];
    let icerik = SAYFA_BOYUTU - hucre.len();
    sayfa[icerik..].copy_from_slice(hucre);
    sayfa[ofset] = 0x0D; // tablo b-tree yaprak sayfası
    sayfa[ofset + 3..ofset + 5].copy_from_slice(&1u16.to_be_bytes()); // hücre sayısı: 1
    sayfa[ofset + 5..ofset + 7].copy_from_slice(&(icerik as u16).to_be_bytes());
    sayfa[ofset + 8..ofset + 10].copy_from_slice(&(icerik as u16).to_be_bytes());
    sayfa
}

/// `sqlite_master` sayfasındaki tek satır: `buyuk` tablosu, kök sayfası 2.
fn master_hucresi() -> Vec<u8> {
    let kayit = kayit_yaz(&[
        Deger::Metin("table".into()),
        Deger::Metin("buyuk".into()),
        Deger::Metin("buyuk".into()),
        Deger::Tam(KOK_SAYFA as i64),
        Deger::Metin("CREATE TABLE buyuk(x)".into()),
    ]);
    let mut hucre = varint_yaz(kayit.len() as i64);
    hucre.extend(varint_yaz(1)); // rowid
    hucre.extend_from_slice(&kayit);
    hucre
}

/// Yük boyutu `bildirilen` olan zehirli tablo hücresini üretir.
///
/// Hücre `varint(bildirilen) + varint(rowid) + yerel parça + 4 bayt varışma işaretçisi`
/// düzenindedir; `sayfa.rs` ile aynı yerel/varışma bölme kuralı kullanılır.
fn zehirli_hucre(bildirilen: i64) -> Vec<u8> {
    let toplam = usize::try_from(bildirilen.max(0)).unwrap_or(0);
    let (yerel, varisma) = yuk_bol(toplam, SayfaTuru::TabloYaprak, SAYFA_BOYUTU as u32);
    let mut hucre = varint_yaz(bildirilen);
    hucre.extend(varint_yaz(1));
    hucre.extend(std::iter::repeat(0x5Au8).take(yerel));
    if varisma {
        hucre.extend_from_slice(&VARISMA_SAYFASI.to_be_bytes());
    }
    hucre
}

/// Zinciri `VARISMA_SAYFASI` sayfasında biten tek varışma sayfası (sonraki sayfa 0).
fn varisma_sayfasi() -> Vec<u8> {
    let mut sayfa = vec![0u8; SAYFA_BOYUTU];
    sayfa[4..].fill(0x11);
    sayfa
}

/// `buyuk` tablosunun tek hücresi `bildirilen` bayt bildiren bir dosya üretir.
fn zehirli_dosya(bildirilen: i64) -> Vec<u8> {
    let mut dosya = birinci_sayfa_taslak(3);
    let master = yaprak_sayfa(&master_hucresi(), MASTER_BASLIK_OFSETI);
    dosya[MASTER_BASLIK_OFSETI..SAYFA_BOYUTU].copy_from_slice(&master[MASTER_BASLIK_OFSETI..]);
    dosya.extend_from_slice(&yaprak_sayfa(&zehirli_hucre(bildirilen), 0));
    dosya.extend_from_slice(&varisma_sayfasi());
    dosya
}

/// Dosyayı geçici dizine yazar ve okuyucuyla tablo satırlarını okumayı dener.
fn oku(yol: &Path) -> Result<Vec<sqlstage::veritabani::Satir>, SahneHata> {
    let db = Veritabani::ac(yol)?;
    db.tablo_satirlari(KOK_SAYFA)
}

#[test]
fn sinir_dis_yuk_boyutu_araci_dusurmez() {
    // Regresyon: `yuk_birlestir` bildirilen boyutu `Vec::with_capacity`ye veriyordu.
    // `i64::MAX` 64-bit'te `usize`'e sorunsuz dönüştüğü için `handle_alloc_error` →
    // `abort` oluyordu. Bu test aracın abort etmediğini, kontrollü hata döndürdüğünü
    // kanıtlar.
    let dizin = match GeciciDizin::yeni("guvenlik-dos") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    for (etiket, bildirilen) in [
        ("i64max", i64::MAX),
        ("pow62", 1i64 << 62),
        ("pow40", 1i64 << 40),
        ("pow31", 1i64 << 31),
    ] {
        let yol = dizin.dosya(&format!("{etiket}.db"));
        if let Err(hata) = std::fs::write(&yol, zehirli_dosya(bildirilen)) {
            panic!("fikstür yazılamadı: {hata}");
        }
        let hata = match oku(&yol) {
            Ok(satirlar) => panic!(
                "{etiket}: zehirli dosya kabul edildi, {} satır okundu",
                satirlar.len()
            ),
            Err(hata) => hata,
        };
        assert!(
            matches!(hata, SahneHata::BozukVarisma { .. }),
            "{etiket}: beklenmeyen hata türü: {hata:?}"
        );
    }
}

#[test]
fn sinir_dis_yuk_boyutu_sema_uzerinden_de_reddedilir() {
    // Aynı zehirli dosya şema okuma yolundan da geçmeli: `sema` yalnızca başlık
    // düzeyinde gezer, yük okuması satır taramasında olur; ikisi de patlamamalıdır.
    let dizin = match GeciciDizin::yeni("guvenlik-sema") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("sema.db");
    if let Err(hata) = std::fs::write(&yol, zehirli_dosya(i64::MAX)) {
        panic!("fikstür yazılamadı: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    // Şema okuma yükü çözmediği için başarılı olmalı.
    let sema = match sqlstage::sema::sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema hatası: {hata}"),
    };
    assert_eq!(sema.tablolar.len(), 1);
    assert_eq!(sema.tablolar[0].kok_sayfa, KOK_SAYFA);
    // Satır taraması zehirli hücreye ulaşınca kontrollü hata vermeli.
    let hata = match db.tablo_satirlari(KOK_SAYFA) {
        Ok(s) => panic!("zehirli satır okundu: {} satır", s.len()),
        Err(hata) => hata,
    };
    assert!(
        matches!(hata, SahneHata::BozukVarisma { .. }),
        "beklenmeyen hata türü: {hata:?}"
    );
}

#[test]
fn negatif_yuk_boyutu_bozuk_kayit_uretir() {
    // Negatif boyut alanı sınır dışıdır: sıfıra çevrilir, yük okunmaz ve kayıt
    // ayrıştırma geçersiz başlıkla karşılaşır. Panik değil, hata beklenir.
    let dizin = match GeciciDizin::yeni("guvenlik-negatif") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("negatif.db");
    if let Err(hata) = std::fs::write(&yol, zehirli_dosya(-1)) {
        panic!("fikstür yazılamadı: {hata}");
    }
    assert!(oku(&yol).is_err());
}
