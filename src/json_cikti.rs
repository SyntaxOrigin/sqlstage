//! Sorgu sonucunun JSON olarak dışa aktarılması.
//!
//! Şema, satırlar ve çalışma istatistikleri birlikte yazılır. Hücre değerleri tipini
//! korur: `NULL` → `null`, tamsayı/gerçek → sayı, metin → dize, blob →
//! `{"$blob": "onaltılık"}` nesnesi.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::hata::SahneHata;
use crate::sql::yurutucu::SorguSonucu;

/// Dışa aktarılan belgenin sürüm numarası.
pub const BELGE_SURUMU: u32 = 1;

/// JSON belgesinin tamamını temsil eden seri hâle getirilebilir yapı.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JsonBelge {
    /// Belge biçiminin sürümü.
    pub surum: u32,
    /// Belgeyi üreten araç.
    pub uretici: String,
    /// Çalıştırılan sorgunun özgün metni.
    pub sorgu: String,
    /// Sonuç kolonlarının adları.
    pub sutunlar: Vec<String>,
    /// Sonuç kolonlarının tipleri.
    pub sutun_tipleri: Vec<String>,
    /// Dönen satır sayısı.
    pub satir_sayisi: usize,
    /// Tablodan okunan toplam satır sayısı.
    pub taranan_satir: u64,
    /// Filtreyi geçen satır sayısı.
    pub eslesen_satir: u64,
    /// Ölçülen yürütme süresi (nanosecond).
    pub sure_ns: u128,
    /// Sonuç satırları; her satır kolon sırasına göre bir dizi.
    pub satirlar: Vec<Vec<Value>>,
}

/// Bir sonuç kümesini belge yapısına çevirir.
///
/// # Hatalar
///
/// Gerçek sayılardan biri JSON sayısı olarak temsil edilemezse hata döner.
pub fn belge_uret(sonuc: &SorguSonucu, sorgu: &str) -> Result<JsonBelge, SahneHata> {
    let mut satirlar = Vec::with_capacity(sonuc.satirlar.len());
    for satir in &sonuc.satirlar {
        let mut hucreler = Vec::with_capacity(satir.len());
        for deger in satir {
            hucreler.push(deger.json_ser().map_err(SahneHata::Json)?);
        }
        satirlar.push(hucreler);
    }
    Ok(JsonBelge {
        surum: BELGE_SURUMU,
        uretici: format!("SQLStage {}", env!("CARGO_PKG_VERSION")),
        sorgu: sorgu.to_string(),
        sutunlar: sonuc.sutunlar.iter().map(|b| b.ad.clone()).collect(),
        sutun_tipleri: sonuc.sutunlar.iter().map(|b| b.tur.clone()).collect(),
        satir_sayisi: sonuc.satirlar.len(),
        taranan_satir: sonuc.taranan_satir,
        eslesen_satir: sonuc.eslesen_satir,
        sure_ns: sonuc.sure_ns,
        satirlar,
    })
}

/// Belgeyi okunabilir (girintili) JSON metnine çevirir.
pub fn metin_uret(belge: &JsonBelge) -> Result<String, SahneHata> {
    serde_json::to_string_pretty(belge).map_err(|hata| SahneHata::Json(hata.to_string()))
}

/// Sonuç kümesini doğrudan JSON metnine çevirir.
pub fn dogrudan(sonuc: &SorguSonucu, sorgu: &str) -> Result<String, SahneHata> {
    metin_uret(&belge_uret(sonuc, sorgu)?)
}

/// Belgeyi bir `serde_json::Value` nesnesine çevirir (testler ve ara işlemler için).
pub fn deger_uret(belge: &JsonBelge) -> Result<Value, SahneHata> {
    serde_json::to_value(belge).map_err(|hata| SahneHata::Json(hata.to_string()))
}

/// Boş bir belge değeri üretir (hata durumunda bile geçerli JSON yazabilmek için).
pub fn bos_deger() -> Value {
    let mut harita = Map::new();
    harita.insert("hata".to_string(), Value::String("bilinmiyor".to_string()));
    Value::Object(harita)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deger::Deger;
    use crate::sql::yurutucu::SutunBasligi;

    fn ornek_sonuc() -> SorguSonucu {
        SorguSonucu {
            sutunlar: vec![
                SutunBasligi {
                    ad: "id".into(),
                    tur: "INTEGER".into(),
                },
                SutunBasligi {
                    ad: "ad".into(),
                    tur: "TEXT".into(),
                },
                SutunBasligi {
                    ad: "resim".into(),
                    tur: "BLOB".into(),
                },
                SutunBasligi {
                    ad: "puan".into(),
                    tur: "REAL".into(),
                },
            ],
            satirlar: vec![vec![
                Deger::Tam(1),
                Deger::Metin("Ali".into()),
                Deger::Blob(vec![0, 255]),
                Deger::Gercek(9.5),
            ]],
            taranan_satir: 3,
            eslesen_satir: 1,
            sure_ns: 1234,
        }
    }

    fn belge(metin_sorgu: &str) -> JsonBelge {
        match belge_uret(&ornek_sonuc(), metin_sorgu) {
            Ok(b) => b,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    fn bos_belge() -> JsonBelge {
        belge_uret(&ornek_sonuc(), "").unwrap_or_else(|_| JsonBelge {
            surum: BELGE_SURUMU,
            uretici: "SQLStage".to_string(),
            sorgu: String::new(),
            sutunlar: Vec::new(),
            sutun_tipleri: Vec::new(),
            satir_sayisi: 0,
            taranan_satir: 0,
            eslesen_satir: 0,
            sure_ns: 0,
            satirlar: Vec::new(),
        })
    }

    #[test]
    fn belge_yapisi_dogru() {
        let b = belge("SELECT * FROM t");
        assert_eq!(b.sutunlar, vec!["id", "ad", "resim", "puan"]);
        assert_eq!(b.satir_sayisi, 1);
        assert_eq!(b.taranan_satir, 3);
        assert_eq!(b.sorgu, "SELECT * FROM t");
        assert!(b.uretici.contains("SQLStage"));
        assert_eq!(b.sutun_tipleri, vec!["INTEGER", "TEXT", "BLOB", "REAL"]);
    }

    #[test]
    fn hucre_tipleri_korunur() {
        let deger = match deger_uret(&belge("x")) {
            Ok(d) => d,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(deger["satirlar"][0][0], Value::from(1));
        assert_eq!(deger["satirlar"][0][1], Value::String("Ali".into()));
        assert_eq!(
            deger["satirlar"][0][2]["$blob"],
            Value::String("00ff".into())
        );
        assert_eq!(deger["satirlar"][0][3], Value::from(9.5));
        assert_eq!(deger["surum"], Value::from(1));
    }

    #[test]
    fn null_json_null_yazilir() {
        let sonuc = SorguSonucu {
            sutunlar: vec![SutunBasligi {
                ad: "a".into(),
                tur: "INTEGER".into(),
            }],
            satirlar: vec![vec![Deger::Null]],
            taranan_satir: 1,
            eslesen_satir: 1,
            sure_ns: 0,
        };
        let metin = match dogrudan(&sonuc, "x") {
            Ok(m) => m,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let cozulmus: Value = match serde_json::from_str(&metin) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cozulmus["satirlar"][0][0], Value::Null);
    }

    #[test]
    fn bos_sonuc_gecerli_json_uretir() {
        let sonuc = SorguSonucu {
            sutunlar: vec![SutunBasligi {
                ad: "a".into(),
                tur: "INTEGER".into(),
            }],
            satirlar: Vec::new(),
            taranan_satir: 5,
            eslesen_satir: 0,
            sure_ns: 7,
        };
        let metin = match dogrudan(&sonuc, "SELECT a FROM t") {
            Ok(m) => m,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let cozulmus: Value = match serde_json::from_str(&metin) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cozulmus["satir_sayisi"], Value::from(0));
        assert_eq!(cozulmus["taranan_satir"], Value::from(5));
    }

    #[test]
    fn bos_deger_gecerli_nesne() {
        let deger = match deger_uret(&bos_belge()) {
            Ok(d) => d,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(deger.is_object());
        assert!(bos_deger().is_object());
    }

    #[test]
    fn metin_uret_girintili_json_verir() {
        let metin = match metin_uret(&belge("x")) {
            Ok(m) => m,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(metin.contains("\n  \"surum\""));
    }
}
