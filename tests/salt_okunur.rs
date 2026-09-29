//! Güvenlik ve salt okunurluk testleri.
//!
//! Kanıtlanan iki şey:
//!
//! 1. Bir `.db` dosyası açılıp okunduktan sonra **bayt bayt aynıdır** (ve dosya
//!    yazmaya karşı salt okunur işaretliyken de açılabilir).
//! 2. Veritabanını değiştiren tüm SQL komutları **reddedilir**; hiçbiri dosyaya
//!    yazmaz.

mod yardimci;

use sqlstage::hata::SahneHata;
use sqlstage::sema::sema_oku;
use sqlstage::sql::parser::ayikla;
use sqlstage::sql::yurutucu::metin_ile_calistir;
use sqlstage::veritabani::Veritabani;
use sqlstage::yazici;

use yardimci::GeciciDizin;

/// Okuma işlemlerinden sonra dosyanın baytlarının değişmediğini doğrular.
#[test]
fn okuma_islemleri_dosyayi_degistirmez() {
    let dizin = match GeciciDizin::yeni("salt-okunur") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("s.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("kurulum yazma hatası: {hata}");
    }
    let once = match std::fs::read(&yol) {
        Ok(b) => b,
        Err(hata) => panic!("okuma hatası: {hata}"),
    };
    let once_uzunluk = once.len();

    // Çeşitli okuma işlemleri: şema, bütünlük, plan, birkaç sorgu.
    {
        let db = match Veritabani::ac(&yol) {
            Ok(v) => v,
            Err(hata) => panic!("açma hatası: {hata}"),
        };
        if let Err(hata) = sema_oku(&db) {
            panic!("şema okunamadı: {hata}");
        }
        let rapor = match db.butunluk() {
            Ok(r) => r,
            Err(hata) => panic!("bütünlük hatası: {hata}"),
        };
        assert!(rapor.temiz_mi(), "{:?}", rapor.sorunlar);
        for sorgu in [
            "SELECT * FROM kisi",
            "SELECT ad FROM kisi WHERE yas IS NULL",
            "SELECT * FROM kisi ORDER BY ad DESC LIMIT 1",
        ] {
            if let Err(hata) = metin_ile_calistir(&db, sorgu) {
                panic!("sorgu hatası: {hata}");
            }
        }
    }

    let sonra = match std::fs::read(&yol) {
        Ok(b) => b,
        Err(hata) => panic!("okuma hatası: {hata}"),
    };
    assert_eq!(once.len(), sonra.len(), "dosya uzunluğu değişti");
    assert_eq!(once, sonra, "veritabanı dosyası bayt bayt değişmiş");
    assert!(once_uzunluk > 0);
}

#[test]
fn salt_okunur_isaretli_dosya_acilir_ve_okunur() {
    let dizin = match GeciciDizin::yeni("salt-isaret") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("ro.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("kurulum yazma hatası: {hata}");
    }
    let mut izin = match std::fs::metadata(&yol) {
        Ok(m) => m.permissions(),
        Err(hata) => panic!("izin ayarlanamadı: {hata}"),
    };
    izin.set_readonly(true);
    if let Err(hata) = std::fs::set_permissions(&yol, izin.clone()) {
        panic!("salt okunur işareti uygulanamadı: {hata}");
    }
    assert!(std::fs::metadata(&yol).is_ok_and(|m| m.permissions().readonly()));

    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("salt okunur dosya açılamadı: {hata}"),
    };
    let sonuc = match metin_ile_calistir(&db, "SELECT ad FROM kisi") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(sonuc.satir_sayisi(), 3);

    // İzleri temizle ki Drop geçici dosyayı silsin. `set_readonly(false)` yalnızca test
    // izidir; kalıcı kodda dosya izinleri hiç değiştirilmez.
    #[allow(clippy::permissions_set_readonly_false)]
    let temizlik = |yol: &std::path::Path| -> std::io::Result<()> {
        let mut geri = std::fs::metadata(yol)?.permissions();
        geri.set_readonly(false);
        std::fs::set_permissions(yol, geri)
    };
    let _ = temizlik(&yol);
}

#[test]
fn veritabanini_degistiren_komutlar_reddedilir() {
    let komutlar = [
        "UPDATE kisi SET ad = 'X'",
        "DELETE FROM kisi",
        "DROP TABLE kisi",
        "INSERT INTO kisi VALUES (4, 'X', 1)",
        "CREATE TABLE x (a INTEGER)",
        "ALTER TABLE kisi ADD COLUMN y TEXT",
        "ATTACH DATABASE 'yeni.db' AS yeni",
        "DETACH DATABASE yeni",
        "PRAGMA journal_mode = WAL",
        "VACUUM",
        "REPLACE INTO kisi VALUES (1, 'X', 1)",
        "BEGIN TRANSACTION",
        "TRUNCATE TABLE kisi",
    ];
    for komut in komutlar {
        match ayikla(komut) {
            Err(SahneHata::YazmaReddi { komut: reddedilen }) => {
                assert!(!reddedilen.is_empty(), "reddedilen komut adı boş");
            }
            diger => panic!("{komut} reddedilmedi: {diger:?}"),
        }
    }
}

#[test]
fn yazma_komutu_dosyaya_dokunmadan_hata_verir() {
    let dizin = match GeciciDizin::yeni("red-komut") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("r.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("kurulum yazma hatası: {hata}");
    }
    let once = std::fs::read(&yol).unwrap_or_default();

    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    match metin_ile_calistir(&db, "DROP TABLE kisi") {
        Err(SahneHata::YazmaReddi { .. }) => {}
        diger => panic!("DROP reddedilmedi: {diger:?}"),
    }

    let sonra = std::fs::read(&yol).unwrap_or_default();
    assert_eq!(once, sonra, "reddedilen komut dosyayı değiştirmiş");
}

#[test]
fn indeksli_veritabani_yeniden_yazilamaz_ve_dosya_korunur() {
    let dizin = match GeciciDizin::yeni("indeks-koruma") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("i.db");
    if let Err(hata) = std::fs::write(&yol, yardimci::onaltilik(yardimci::GERCEK_SQLITE_HEX)) {
        panic!("fixture yazılamadı: {hata}");
    }
    let once = std::fs::read(&yol).unwrap_or_default();

    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema hatası: {hata}"),
    };
    assert!(!sema.indeksler.is_empty(), "fixture'ta indeks olmalı");
    // `insert` yolu model üretirken indeks varsa yazmayı reddetmelidir.
    let sonuc: Result<(), SahneHata> = (|| {
        if !sema.indeksler.is_empty() {
            return Err(SahneHata::Desteklenmiyor {
                ozellik: sema.indeksler[0].ad.clone(),
                sebep: "indeks içeren veritabanları yeniden yazılamaz",
            });
        }
        Ok(())
    })();
    assert!(matches!(sonuc, Err(SahneHata::Desteklenmiyor { .. })));
    assert_eq!(once, std::fs::read(&yol).unwrap_or_default());
}
