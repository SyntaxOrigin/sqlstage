//! Uçtan uca entegrasyon testleri: örnek veri tabanı üretimi, şema okuma, sorgu
//! çalıştırma, JSON dışa aktarma ve sorgu geçmişi.
//!
//! Bu dosya CLI'nin yaptığı işi kütüphane üzerinden uçtan uca doğrular; her alt komutun
//! karşılığı gelen bir işlev çağrılır.

mod yardimci;

use sqlstage::deger::Deger;
use sqlstage::gecmis::{GecmisKaydi, VARSAYILAN_DOSYA_ADI};
use sqlstage::izgara::IzgaraAyari;
use sqlstage::json_cikti;
use sqlstage::sema::sema_oku;
use sqlstage::sql::parser::ayikla;
use sqlstage::sql::yurutucu::{calistir, metin_ile_calistir};
use sqlstage::veritabani::Veritabani;
use sqlstage::yazici::{self, TabloYazimi};
use sqlstage::{diyagram, gecmis, izgara, ornek, plan};

use yardimci::GeciciDizin;

#[test]
fn ornek_veri_tabani_uretilip_okunur() {
    let dizin = match GeciciDizin::yeni("ornek") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("ornek.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &ornek::ornek_tablolar()) {
        panic!("örnek veri tabanı yazılamadı: {hata}");
    }
    assert!(yol.exists());

    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("örnek veri tabanı açılamadı: {hata}"),
    };
    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema okunamadı: {hata}"),
    };
    assert_eq!(sema.tablolar.len(), 3);
    let toplam: u64 = sema.tablolar.iter().map(|t| t.satir_sayisi).sum();
    assert_eq!(toplam, 28, "8 ürün + 8 müşteri + 12 sipariş");
}

#[test]
fn sema_diyagraminda_tablolar_ve_iliskiler_gorunur() {
    let dizin = match GeciciDizin::yeni("diyagram") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("d.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &ornek::ornek_tablolar()) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema hatası: {hata}"),
    };
    let metin = diyagram::diyagram_uret(&sema);
    assert!(metin.contains("siparisler"));
    assert!(metin.contains("FOREIGN") || metin.contains("[N]"));
    let iliskiler = diyagram::iliski_satirlari(&sema);
    assert_eq!(iliskiler.len(), 2, "iki yabancı anahtar var");
}

#[test]
fn tum_filtre_operatorleri_uygulanir() {
    let dizin = match GeciciDizin::yeni("filtre") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("f.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };

    let say = |sorgu: &str| -> usize {
        match metin_ile_calistir(&db, sorgu) {
            Ok(sonuc) => sonuc.satir_sayisi(),
            Err(hata) => panic!("sorgu çalışmadı ({sorgu}): {hata}"),
        }
    };

    assert_eq!(say("SELECT * FROM kisi"), 3);
    assert_eq!(say("SELECT * FROM kisi WHERE yas > 30"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE yas >= 30"), 2);
    assert_eq!(say("SELECT * FROM kisi WHERE yas < 41"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE yas <= 30"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE yas = 30"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE yas != 30"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE ad LIKE 'A%'"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE ad LIKE '%ey%'"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE ad LIKE '_eli'"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE ad NOT LIKE 'A%'"), 2);
    assert_eq!(say("SELECT * FROM kisi WHERE yas IS NULL"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE yas IS NOT NULL"), 2);
    assert_eq!(say("SELECT * FROM kisi WHERE yas > 20 AND ad = 'Ali'"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE yas > 100 OR ad = 'Ali'"), 1);
    assert_eq!(say("SELECT * FROM kisi WHERE NOT (ad = 'Ali')"), 2);
    // NULL tuzağı: ayrıştırıcı `= NULL` yazımını `IS NULL` kullanmaya yönlendirip
    // reddeder; bu, "sessizce boş sonuç dönme" yerine kullanıcıyı uyarır.
    match ayikla("SELECT * FROM kisi WHERE yas = NULL") {
        Err(sqlstage::hata::SahneHata::SorguHatasi { mesaj, .. }) => {
            assert!(mesaj.contains("IS NULL"), "{mesaj}");
        }
        diger => panic!("`= NULL` reddedilmedi: {diger:?}"),
    }
    // Yine de üç değerli mantık: NULL sütunu hiçbir karşılaştırmada eşleşmez.
    let say_filtre = |sorgu: &str| -> usize {
        match metin_ile_calistir(&db, sorgu) {
            Ok(sonuc) => sonuc.satir_sayisi(),
            Err(hata) => panic!("sorgu çalışmadı ({sorgu}): {hata}"),
        }
    };
    assert_eq!(say_filtre("SELECT * FROM kisi WHERE yas > 29"), 2);
    assert_eq!(say_filtre("SELECT * FROM kisi WHERE yas > 40"), 1);
    assert_eq!(
        say_filtre("SELECT * FROM kisi WHERE yas = 30 AND ad IS NULL"),
        0
    );
}

#[test]
fn limit_siralama_ve_takma_ad() {
    let dizin = match GeciciDizin::yeni("siralama") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("s.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };

    let sonuc = match metin_ile_calistir(&db, "SELECT ad FROM kisi ORDER BY yas DESC") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    let adlar: Vec<String> = sonuc.satirlar.iter().map(|s| s[0].gosterim()).collect();
    // NULL en küçük olduğu için DESC sıralamada en sona düşer.
    assert_eq!(adlar, vec!["Zeynep", "Ali", "Veli"]);

    let kisit = match metin_ile_calistir(&db, "SELECT ad FROM kisi ORDER BY yas LIMIT 1") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(kisit.satir_sayisi(), 1);
    assert_eq!(kisit.satirlar[0][0].gosterim(), "Veli");

    let ofsetli = match metin_ile_calistir(&db, "SELECT ad FROM kisi ORDER BY id LIMIT 1 OFFSET 1")
    {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(ofsetli.satirlar[0][0].gosterim(), "Veli");

    let takma = match metin_ile_calistir(&db, "SELECT ad AS isim FROM kisi LIMIT 1") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(takma.sutunlar[0].ad, "isim");
}

#[test]
fn sonuc_izgarasi_sigmayan_kolonu_kisaltir() {
    let dizin = match GeciciDizin::yeni("izgara") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("i.db");
    let uzun = "c".repeat(300);
    let tablo = TabloYazimi {
        ad: "not".to_string(),
        sql: "CREATE TABLE not(id INTEGER, metin TEXT)".to_string(),
        satirlar: vec![(1, vec![Deger::Tam(1), Deger::Metin(uzun)])],
    };
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[tablo]) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sonuc = match metin_ile_calistir(&db, "SELECT * FROM not") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    let metin = izgara::ciz(&sonuc, &IzgaraAyari::yeni(12));
    assert!(metin.contains('~'), "kısaltma işareti bulunmalı");
    for satir in metin.lines() {
        assert!(satir.chars().count() <= 40, "satır çok uzun: {satir}");
    }
}

#[test]
fn json_disa_aktarim_tipleri_korur() {
    let dizin = match GeciciDizin::yeni("json") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("j.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sonuc = match metin_ile_calistir(&db, "SELECT * FROM kisi") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    let metin = match json_cikti::dogrudan(&sonuc, "SELECT * FROM kisi") {
        Ok(m) => m,
        Err(hata) => panic!("JSON üretilemedi: {hata}"),
    };
    let deger: serde_json::Value = match serde_json::from_str(&metin) {
        Ok(v) => v,
        Err(hata) => panic!("JSON ayrıştırılamadı: {hata}"),
    };
    assert_eq!(deger["satir_sayisi"], serde_json::Value::from(3));
    assert_eq!(
        deger["satirlar"][0][1],
        serde_json::Value::String("Ali".into())
    );
    assert_eq!(deger["satirlar"][1][2], serde_json::Value::Null);
    assert!(deger["uretici"].as_str().unwrap_or("").contains("SQLStage"));
}

#[test]
fn sorgu_plani_olculmedi_uyarisi_icerir() {
    let dizin = match GeciciDizin::yeni("plan") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("p.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sorgu = match ayikla("SELECT * FROM kisi WHERE yas > 20 ORDER BY ad LIMIT 2") {
        Ok(s) => s,
        Err(hata) => panic!("ayrıştırma hatası: {hata}"),
    };
    let metin = match plan::plan_metni(&db, &sorgu) {
        Ok(m) => m,
        Err(hata) => panic!("plan hatası: {hata}"),
    };
    assert!(metin.contains("ÖLÇÜLMEDİ"));
    assert!(metin.contains("TABLO TARAMA"));
    assert!(metin.contains("kisi"));
    assert!(metin.contains("EXPLAIN QUERY PLAN"));
}

#[test]
fn create_ile_tablo_olusur_ve_insert_ile_satir_eklenir() {
    let dizin = match GeciciDizin::yeni("create") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("yeni.db");

    // create: tek kolonlu tablo oluştur
    let tablolar = vec![TabloYazimi {
        ad: "not".to_string(),
        sql: "CREATE TABLE not(id INTEGER PRIMARY KEY, metin TEXT NOT NULL)".to_string(),
        satirlar: Vec::new(),
    }];
    if let Err(hata) = yazici::veritabani_yaz(&yol, &tablolar) {
        panic!("create yazma hatası: {hata}");
    }

    // insert: satır ekle (dosya baştan yazılır)
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema hatası: {hata}"),
    };
    let mut model: Vec<TabloYazimi> = Vec::new();
    for tablo in &sema.tablolar {
        let satirlar = match db.tablo_satirlari(tablo.kok_sayfa) {
            Ok(s) => s
                .into_iter()
                .map(|s| (s.satir_kimligi, s.degerler))
                .collect(),
            Err(hata) => panic!("satır okunamadı: {hata}"),
        };
        model.push(TabloYazimi {
            ad: tablo.ad.clone(),
            sql: tablo.sql.clone(),
            satirlar,
        });
    }
    model[0]
        .satirlar
        .push((1, vec![Deger::Tam(1), Deger::Metin("ilk not".into())]));
    if let Err(hata) = yazici::dosya_guncelle(
        &yol,
        &yazici::veritabani_goruntusu(&model).unwrap_or_default(),
    ) {
        panic!("insert yazma hatası: {hata}");
    }

    let db2 = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("yeniden açma hatası: {hata}"),
    };
    let sonuc = match metin_ile_calistir(&db2, "SELECT metin FROM not") {
        Ok(s) => s,
        Err(hata) => panic!("sorgu hatası: {hata}"),
    };
    assert_eq!(sonuc.satir_sayisi(), 1);
    assert_eq!(sonuc.satirlar[0][0].gosterim(), "ilk not");
}

#[test]
fn cok_buyuk_kayit_varisma_sayfalarina_yazilir_ve_oku() {
    let dizin = match GeciciDizin::yeni("varisma") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("v.db");
    let devasa = "Ş".repeat(60_000); // 120 000 bayt: tek sayfaya sığmaz.
    let tablo = TabloYazimi {
        ad: "buyuk".to_string(),
        sql: "CREATE TABLE buyuk(id INTEGER PRIMARY KEY, metin TEXT)".to_string(),
        satirlar: vec![(1, vec![Deger::Tam(1), Deger::Metin(devasa.clone())])],
    };
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[tablo]) {
        panic!("yazma hatası: {hata}");
    }
    assert!(
        yol.metadata().map_or(0, |m| m.len()) > 60_000,
        "dosya büyük olmalı"
    );

    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sema = match sema_oku(&db) {
        Ok(s) => s,
        Err(hata) => panic!("şema hatası: {hata}"),
    };
    let kok = sema.tablolar[0].kok_sayfa;
    let satirlar = match db.tablo_satirlari(kok) {
        Ok(s) => s,
        Err(hata) => panic!("varışma okunamadı: {hata}"),
    };
    assert_eq!(satirlar.len(), 1);
    match satirlar[0].deger(1) {
        Some(Deger::Metin(metin)) => assert_eq!(metin, &devasa),
        diger => panic!("beklenmeyen değer: {diger:?}"),
    }
}

#[test]
fn gecmis_kayitlari_yazilir_ve_tekrar_calistirilir() {
    let dizin = match GeciciDizin::yeni("gecmis") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("g.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("yazma hatası: {hata}");
    }
    let gecis = dizin.dosya(VARSAYILAN_DOSYA_ADI);

    let kayit = GecmisKaydi::basarili(&yol.display().to_string(), "SELECT * FROM kisi", 42, 3);
    if let Err(hata) = gecmis::kaydet(&gecis, &kayit) {
        panic!("geçmişe yazılamadı: {hata}");
    }
    let kayitlar = match gecmis::oku(&gecis) {
        Ok(k) => k,
        Err(hata) => panic!("geçmiş okunamadı: {hata}"),
    };
    assert_eq!(kayitlar.len(), 1);

    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let tekrar = gecmis::kayit_getir(&gecis, 1)
        .unwrap_or_else(|h| GecmisKaydi::basarisiz("", "", &h.to_string()));
    let sonuc = match metin_ile_calistir(&db, &tekrar.sorgu) {
        Ok(s) => s,
        Err(hata) => panic!("geçmişten çalıştırma hatası: {hata}"),
    };
    assert_eq!(sonuc.satir_sayisi(), 3);
}

#[test]
fn yurutucu_ayristirilmis_sorgu_dogrudan_calistirilir() {
    let dizin = match GeciciDizin::yeni("dogrudan") {
        Ok(d) => d,
        Err(hata) => panic!("geçici dizin oluşturulamadı: {hata}"),
    };
    let yol = dizin.dosya("d.db");
    if let Err(hata) = yazici::veritabani_yaz(&yol, &[yardimci::kisi_tablasi()]) {
        panic!("yazma hatası: {hata}");
    }
    let db = match Veritabani::ac(&yol) {
        Ok(v) => v,
        Err(hata) => panic!("açma hatası: {hata}"),
    };
    let sorgu = match ayikla("SELECT ad FROM kisi WHERE yas IS NOT NULL ORDER BY ad") {
        Ok(s) => s,
        Err(hata) => panic!("ayrıştırma hatası: {hata}"),
    };
    let sonuc = match calistir(&db, &sorgu) {
        Ok(s) => s,
        Err(hata) => panic!("çalıştırma hatası: {hata}"),
    };
    assert_eq!(sonuc.satir_sayisi(), 2);
    assert_eq!(sonuc.taranan_satir, 3);
    assert_eq!(sonuc.eslesen_satir, 2);
}
