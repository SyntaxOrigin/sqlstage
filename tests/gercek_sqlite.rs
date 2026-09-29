//! Üçüncü taraf (SQLite 3.41) tarafından üretilmiş **gerçek** veritabanı dosyalarını
//! okuma uyumluluğu.
//!
//! Bu testler kendi yazıcımızın çıktısını değil, bağımsız bir uygulamanın ürettiği
//! dosyaları kullanır. Amaç, b-tree ve kayıt ayrıştırıcımızın yalnızca kendi
//! varsayımlarımızı doğrulamakla kalmayıp gerçek biçimle de uyumlu olduğunu göstermektir.
//!
//! Fixture üretimi: Python `sqlite3` modülü, `PRAGMA page_size=512`, `VACUUM`.

mod yardimci;

use sqlstage::baslik::MetinKodlamasi;
use sqlstage::deger::Deger;
use sqlstage::sema::sema_oku;
use sqlstage::sql::yurutucu::metin_ile_calistir;
use sqlstage::veritabani::Veritabani;

use yardimci::GeciciDizin;

fn fixture(yol: &std::path::Path, hex: &str) -> Veritabani {
    if let Err(hata) = std::fs::write(yol, yardimci::onaltilik(hex)) {
        panic!("fixture yazılamadı: {hata}");
    }
    match Veritabani::ac(yol) {
        Ok(db) => db,
        Err(hata) => panic!("gerçek veritabanı açılamadı: {hata}"),
    }
}

#[test]
fn gercek_sqlite_dosyasi_basi_dogrulanir() {
    let dizin = match GeciciDizin::yeni("gercel-baslik") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let db = fixture(&dizin.dosya("g.db"), yardimci::GERCEK_SQLITE_HEX);
    assert_eq!(db.baslik().sayfa_boyutu, 512);
    assert_eq!(db.baslik().kodlama, MetinKodlamasi::Utf8);
    assert_eq!(db.baslik().rezerve, 0);
    assert!(!db.wal_etkin_mi());
    assert_eq!(db.sayfa_sayisi(), 5);
}

#[test]
fn gercek_sqlite_dosyasinda_sema_okunur() {
    let dizin = match GeciciDizin::yeni("gercel-sema") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let db = fixture(&dizin.dosya("g.db"), yardimci::GERCEK_SQLITE_HEX);
    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema okunamadı: {hata}"),
    };
    let kisi = match sema.tablo("kisi") {
        Some(t) => t,
        None => panic!("kisi tablosu yok"),
    };
    assert_eq!(kisi.kolonlar.len(), 4);
    assert_eq!(kisi.birincil_anahtar(), vec!["id"]);
    assert!(kisi.kolon("ad").map(|k| k.not_null).unwrap_or(false));
    assert_eq!(kisi.satir_sayisi, 3);

    let sehir = match sema.tablo("sehir") {
        Some(t) => t,
        None => panic!("sehir tablosu yok"),
    };
    assert_eq!(sehir.satir_sayisi, 2);
    assert_eq!(sehir.birincil_anahtar(), vec!["ad"]);

    // sqlite_autoindex_* girdileri indeks olarak sınıflanır, tablo olarak değil.
    assert!(
        sema.indeksler.iter().any(|i| i.ad == "kisi_yas"),
        "kisi_yas indeksi okunmalı"
    );
    assert_eq!(sema.tablolar.len(), 2);
}

#[test]
fn gercek_sqlite_dosyasinda_sorgu_calisir() {
    let dizin = match GeciciDizin::yeni("gercel-sorgu") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let db = fixture(&dizin.dosya("g.db"), yardimci::GERCEK_SQLITE_HEX);

    let tum = match metin_ile_calistir(&db, "SELECT id, ad, yas, puan FROM kisi") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(tum.satir_sayisi(), 3);
    assert_eq!(tum.satirlar[0][1], Deger::Metin("Ali".into()));
    assert_eq!(tum.satirlar[0][3], Deger::Gercek(9.5));
    // İkinci satırda yas NULL'dur: üç değerli mantık devreye girmeli.
    assert_eq!(tum.satirlar[1][2], Deger::Null);
    assert!(tum.satirlar[1][2].null_mu());

    let nullsuz = match metin_ile_calistir(&db, "SELECT id FROM kisi WHERE yas IS NOT NULL") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(nullsuz.satir_sayisi(), 2);

    let like = match metin_ile_calistir(&db, "SELECT ad FROM kisi WHERE ad LIKE '%ey%'") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(like.satir_sayisi(), 1);

    let sirali = match metin_ile_calistir(&db, "SELECT ad FROM kisi ORDER BY ad DESC") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    let adlar: Vec<String> = sirali.satirlar.iter().map(|s| s[0].gosterim()).collect();
    assert_eq!(adlar, vec!["Zeynep", "Veli", "Ali"]);
}

#[test]
fn gercek_sqlite_dosyasi_butunluk_denetiminden_gecer() {
    let dizin = match GeciciDizin::yeni("gercel-butunluk") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let db = fixture(&dizin.dosya("g.db"), yardimci::GERCEK_SQLITE_HEX);
    let rapor = match db.butunluk() {
        Ok(r) => r,
        Err(hata) => panic!("bütünlük hatası: {hata}"),
    };
    assert!(rapor.temiz_mi(), "{:?}", rapor.sorunlar);
    assert!(rapor.denetlenen_sayfa >= 3);
}

#[test]
fn utf16_kodlamali_gercek_dosya_okunur() {
    let dizin = match GeciciDizin::yeni("utf16") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let db = fixture(&dizin.dosya("u.db"), yardimci::UTF16_SQLITE_HEX);
    assert_eq!(db.baslik().kodlama, MetinKodlamasi::Utf16Le);

    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema okunamadı: {hata}"),
    };
    let notlar = match sema.tablo("notlar") {
        Some(t) => t,
        None => panic!("notlar tablosu yok"),
    };
    assert_eq!(notlar.satir_sayisi, 2);
    assert_eq!(notlar.kolonlar.len(), 2);
    assert_eq!(notlar.kolonlar[1].tur.ad(), "TEXT");

    let sonuc = match metin_ile_calistir(&db, "SELECT metin FROM notlar ORDER BY id") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(sonuc.satir_sayisi(), 2);
    assert_eq!(sonuc.satirlar[0][0], Deger::Metin("Merhaba".into()));
    assert_eq!(sonuc.satirlar[1][0], Deger::Metin("Dünya".into()));
}
