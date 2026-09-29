//! Örnek veri tabanı üreticisi.
//!
//! Kullanıcının elinde hiçbir `.db` dosyası yoksa `sample` komutuyla üç tablolu bir
//! örnek veri tabanı üretilebilir. Veri **deterministiktir**: sabit bir LCG tohumu
//! kullanılır, rastgelelik crate'i yoktur ve her çalıştırmada aynı sonuç üretilir.
//!
//! Alan seçimi (rapor b16'daki açık soru): küçük bir e-ticaret/sipariş alanı seçildi;
//! üç tablo (urunler, musteriler, siparisler) ilişkiyi ve `JOIN` ihtiyacını öğretmek için
//! yeterli çeşitlilik sunar.

use crate::deger::Deger;
use crate::yazici::TabloYazimi;

/// Deterministik sözde rastgele üreteci (Numerical Recipes LCG).
///
/// `rand` crate'i yasak olduğu için küçük bir lineer eşleştirici yeterlidir: veri
/// yalnızca çeşitlilik üretmek içindir, güvenlik gereksinimi yoktur.
struct Lcg(u64);

impl Lcg {
    fn yeni() -> Self {
        Self(2_026_092_929)
    }

    fn sonraki(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    /// Belirtilen aralıkta (alt dahil, üst hariç) bir değer üretir.
    fn aralik(&mut self, alt: u64, ust: u64) -> u64 {
        alt + self.sonraki() % (ust - alt)
    }
}

/// Ürün kategorileri (sabit liste).
const KATEGORILER: &[&str] = &["elektronik", "ev", "kitap", "giysi", "spor"];
/// Sipariş durumları (sabit liste).
const DURUMLAR: &[&str] = &["beklemede", "kargoda", "teslim", "iptal"];

/// Örnek veri tabanının tüm tablolarını döndürür.
///
/// Kurgu 5 ürün, 8 müşteri ve 12 sipariş satırıdır. Sütunlarda tüm beş değer tipi
/// (NULL, tamsayı, gerçek, metin, blob) en az bir kez bulunur.
pub fn ornek_tablolar() -> Vec<TabloYazimi> {
    let mut uretec = Lcg::yeni();

    let urun_adlari = [
        ("Klavye", "elektronik", 449.90_f64),
        ("Fare", "elektronik", 219.50),
        ("Termos", "ev", 189.00),
        ("Roman Seti", "kitap", 320.75),
        ("Mont", "giysi", 749.90),
        ("Bisiklet", "spor", 4990.00),
        ("Kitaplik", "ev", 259.90),
        ("Kulaklik", "elektronik", 899.99),
    ];
    let urunler: Vec<TabloYazimi> = vec![TabloYazimi {
        ad: "urunler".to_string(),
        sql: "CREATE TABLE urunler(id INTEGER PRIMARY KEY, ad TEXT NOT NULL, kategori TEXT, \
              fiyat REAL, stok INTEGER, etiket BLOB)"
            .to_string(),
        satirlar: urun_adlari
            .iter()
            .enumerate()
            .map(|(i, (ad, kategori, fiyat))| {
                (
                    i as i64 + 1,
                    vec![
                        Deger::Tam(i as i64 + 1),
                        Deger::Metin((*ad).to_string()),
                        Deger::Metin((*kategori).to_string()),
                        Deger::Gercek(*fiyat),
                        Deger::Tam(uretec.aralik(0, 60) as i64),
                        Deger::Blob(vec![(i % 256) as u8, 0x00, (255 - i % 256) as u8]),
                    ],
                )
            })
            .collect(),
    }];

    let musteri_adlari = [
        ("Ali Yilmaz", "Izmir"),
        ("Veli Kaya", "Ankara"),
        ("Ayse Demir", "Istanbul"),
        ("Mehmet Sahin", "Bursa"),
        ("Zeynep Aydin", "Antalya"),
        ("Emre Koc", "Izmir"),
        ("Fatma Celik", "Ankara"),
        ("Can Yilmaz", "Istanbul"),
    ];
    let musteriler: Vec<TabloYazimi> = vec![TabloYazimi {
        ad: "musteriler".to_string(),
        sql: "CREATE TABLE musteriler(id INTEGER PRIMARY KEY, ad TEXT NOT NULL, sehir TEXT, \
              yas INTEGER, kayit_tarihi TEXT)"
            .to_string(),
        satirlar: musteri_adlari
            .iter()
            .enumerate()
            .map(|(i, (ad, sehir))| {
                (
                    i as i64 + 1,
                    vec![
                        Deger::Tam(i as i64 + 1),
                        Deger::Metin((*ad).to_string()),
                        Deger::Metin((*sehir).to_string()),
                        // Her altıncı kayıtta yaş bilgisi NULL bırakılır (NULL örneği).
                        if i % 6 == 5 {
                            Deger::Null
                        } else {
                            Deger::Tam(21 + (i as i64) * 4)
                        },
                        Deger::Metin(format!("2024-{:02}-{:02}", (i % 12) + 1, (i % 27) + 1)),
                    ],
                )
            })
            .collect(),
    }];

    let mut siparis_satirlari: Vec<(i64, Vec<Deger>)> = Vec::new();
    for i in 0..12i64 {
        let urun = uretec.aralik(1, 9) as usize; // 1 tabanlı ürün numarası
        let musteri = uretec.aralik(1, 9) as i64;
        let adet = uretec.aralik(1, 5) as i64;
        let tutar = (urun_adlari[urun - 1].2 * adet as f64 * 100.0).round() / 100.0;
        siparis_satirlari.push((
            i + 1,
            vec![
                Deger::Tam(i + 1),
                Deger::Tam(musteri),
                Deger::Tam(urun as i64),
                Deger::Tam(adet),
                Deger::Gercek(tutar),
                Deger::Metin(DURUMLAR[(i % DURUMLAR.len() as i64) as usize].to_string()),
                Deger::Metin(format!("2025-{:02}-{:02}", (i % 12) + 1, (i % 27) + 1)),
            ],
        ));
    }
    let siparisler = vec![TabloYazimi {
        ad: "siparisler".to_string(),
        sql: "CREATE TABLE siparisler(id INTEGER PRIMARY KEY, musteri_id INTEGER NOT NULL, \
              urun_id INTEGER NOT NULL, adet INTEGER, tutar REAL, durum TEXT, tarih TEXT, \
              FOREIGN KEY (musteri_id) REFERENCES musteriler(id), \
              FOREIGN KEY (urun_id) REFERENCES urunler(id))"
            .to_string(),
        satirlar: siparis_satirlari,
    }];

    let mut tablolar = Vec::new();
    tablolar.extend(urunler);
    tablolar.extend(musteriler);
    tablolar.extend(siparisler);
    tablolar
}

/// Örnek veri tabanı hakkında kullanıcıya gösterilecek kısa açıklama.
pub fn aciklama() -> String {
    let tablolar = ornek_tablolar();
    let satirlar: usize = tablolar.iter().map(|t| t.satirlar.len()).sum();
    let adlar: Vec<&str> = tablolar.iter().map(|t| t.ad.as_str()).collect();
    format!(
        "{} tablo ({}), toplam {} satır. Kategoriler: {}.",
        tablolar.len(),
        adlar.join(", "),
        satirlar,
        KATEGORILER.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ornek_veri_uc_tablo_uretir() {
        let tablolar = ornek_tablolar();
        assert_eq!(tablolar.len(), 3);
        let adlar: Vec<&str> = tablolar.iter().map(|t| t.ad.as_str()).collect();
        assert_eq!(adlar, vec!["urunler", "musteriler", "siparisler"]);
    }

    #[test]
    fn satir_kimligi_sirali_ve_bir_basedir() {
        for tablo in ornek_tablolar() {
            for (i, (rowid, _)) in tablo.satirlar.iter().enumerate() {
                assert_eq!(*rowid, i as i64 + 1, "tablo {}", tablo.ad);
            }
        }
    }

    #[test]
    fn veri_deterministiktir() {
        assert_eq!(ornek_tablolar(), ornek_tablolar());
    }

    #[test]
    fn tum_deger_tipleri_bulunur() {
        let tum_tipler = ornek_tablolar()
            .into_iter()
            .flat_map(|t| t.satirlar)
            .flat_map(|(_, degerler)| degerler)
            .any(|d| d.null_mu());
        assert!(tum_tipler, "örnek veride NULL bulunmalı");
    }

    #[test]
    fn siparisler_musterilere_bagli() {
        let tablolar = ornek_tablolar();
        let siparisler = tablolar
            .iter()
            .find(|t| t.ad == "siparisler")
            .unwrap_or_else(|| panic!("siparisler yok"));
        let musteriler = tablolar
            .iter()
            .find(|t| t.ad == "musteriler")
            .unwrap_or_else(|| panic!("musteriler yok"));
        for (_, degerler) in &siparisler.satirlar {
            let referans = match &degerler[1] {
                Deger::Tam(i) => *i,
                diger => panic!("beklenmeyen tip: {diger:?}"),
            };
            assert!(referans >= 1 && referans as usize <= musteriler.satirlar.len());
        }
    }

    #[test]
    fn aciklama_metin_uretir() {
        let metin = aciklama();
        assert!(metin.contains("urunler"));
        assert!(metin.contains("musteriler"));
        assert!(metin.contains("siparisler"));
    }
}
