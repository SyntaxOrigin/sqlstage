//! SQLite dinamik değer tipleri.
//!
//! SQLite bir sütun için statik tip tutmaz; tip bilgisi `CREATE TABLE` bildiriminden
//! gelir, depolanan değer ise serial type ile belirlenir. Bu modül, ayrıştırılmış tek bir
//! hücre değerini temsil eder ve tüm karşılaştırma sıralamasını burada toplar.

use std::cmp::Ordering;
use std::fmt;

use serde::ser::{Serialize, Serializer};

/// Tek bir hücrenin sahip olabileceği beş değer tipinden biri.
#[derive(Debug, Clone, PartialEq)]
pub enum Deger {
    /// SQL `NULL`; hiçbir karşılaştırmada eşitlik ölçütü olarak `true` dönmez.
    Null,
    /// 64 bit işaretli tamsayı.
    Tam(i64),
    /// IEEE-754 çift duyarlılıklı gerçek sayı.
    Gercek(f64),
    /// UTF-8 metin (başlıktaki kodlamaya göre çözülmüş hâlde).
    Metin(String),
    /// Bayt dizisi.
    Blob(Vec<u8>),
}

impl Deger {
    /// Değerin tür etiketini döndürür (`NULL`, `INTEGER`, `REAL`, `TEXT`, `BLOB`).
    pub fn tur(&self) -> &'static str {
        match self {
            Self::Null => "NULL",
            Self::Tam(_) => "INTEGER",
            Self::Gercek(_) => "REAL",
            Self::Metin(_) => "TEXT",
            Self::Blob(_) => "BLOB",
        }
    }

    /// Değerin `NULL` olup olmadığını döndürür.
    pub fn null_mu(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Metin karşılığını döndürür; metin değilse `None` verir.
    pub fn metin(&self) -> Option<&str> {
        match self {
            Self::Metin(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Terminal ızgarada gösterilecek kısa metin temsilini döndürür.
    ///
    /// `NULL` için `NULL`, blob için `x'..'` onaltılık gösterim kullanılır; diğer
    /// tipler Rust `Debug`-benzeri bir yazımla basılır.
    pub fn gosterim(&self) -> String {
        match self {
            Self::Null => "NULL".to_string(),
            Self::Metin(s) => s.clone(),
            Self::Tam(i) => i.to_string(),
            Self::Gercek(f) => format_sayi(*f),
            Self::Blob(b) => format!(
                "x'{}'",
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            ),
        }
    }

    /// SQLite sıralama derecesi: `NULL` en küçük, sonra sayılar, metin, blob.
    ///
    /// Bu sıra `ORDER BY` ve `MIN`/`MAX` karşılaştırmalarında SQLite ile aynıdır.
    pub fn siralama_sinifi(&self) -> u8 {
        match self {
            Self::Null => 0,
            Self::Tam(_) | Self::Gercek(_) => 1,
            Self::Metin(_) => 2,
            Self::Blob(_) => 3,
        }
    }

    /// İki değeri SQLite sırasına göre karşılaştırır.
    ///
    /// Metinler bayt bayt (`BINARY` harmanlama) karşılaştırılır; SQLite'in varsayılan
    /// harmanlaması da budur.
    pub fn karsilastir(&self, diger: &Self) -> Ordering {
        match (self, diger) {
            (Self::Tam(a), Self::Tam(b)) => a.cmp(b),
            (Self::Gercek(a), Self::Gercek(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Self::Tam(a), Self::Gercek(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (Self::Gercek(a), Self::Tam(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }
            (Self::Metin(a), Self::Metin(b)) => a.as_bytes().cmp(b.as_bytes()),
            (Self::Blob(a), Self::Blob(b)) => a.cmp(b),
            (a, b) => a.siralama_sinifi().cmp(&b.siralama_sinifi()),
        }
    }

    /// İki sayısal değerin toplamını verir; sayısal olmayanlarda `None` döner.
    pub fn sayisal_toplam(&self) -> Option<f64> {
        match self {
            Self::Tam(i) => Some(*i as f64),
            Self::Gercek(f) => Some(*f),
            _ => None,
        }
    }

    /// Değerin JSON temsilini üretir.
    ///
    /// Blob, saf metinden ayırt edilebilmesi için `{"$blob": "hex"}` nesnesi olarak
    /// yazılır; böylece JSON tüketicisi tipi geri dönüştürebilir.
    pub fn json_ser(&self) -> Result<serde_json::Value, String> {
        match self {
            Self::Null => Ok(serde_json::Value::Null),
            Self::Tam(i) => Ok(serde_json::Value::from(*i)),
            Self::Gercek(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| format!("{f} değeri JSON sayısı olarak temsil edilemedi")),
            Self::Metin(s) => Ok(serde_json::Value::String(s.clone())),
            Self::Blob(b) => {
                let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
                let mut harita = serde_json::Map::new();
                harita.insert("$blob".to_string(), serde_json::Value::String(hex));
                Ok(serde_json::Value::Object(harita))
            }
        }
    }
}

impl Serialize for Deger {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.json_ser() {
            Ok(deger) => deger.serialize(serializer),
            Err(mesaj) => serializer.serialize_str(&mesaj),
        }
    }
}

impl fmt::Display for Deger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.gosterim())
    }
}

/// Gerçek sayıyı SQLite'ın yaptığı gibi yazar: tamsayıya yakın değerler `1.0` biçiminde.
fn format_sayi(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Inf" } else { "-Inf" }.to_string();
    }
    if f == f.trunc() && f.abs() < 1e15 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tur_etiketleri_dogru() {
        assert_eq!(Deger::Null.tur(), "NULL");
        assert_eq!(Deger::Tam(1).tur(), "INTEGER");
        assert_eq!(Deger::Gercek(1.5).tur(), "REAL");
        assert_eq!(Deger::Metin("a".into()).tur(), "TEXT");
        assert_eq!(Deger::Blob(vec![1]).tur(), "BLOB");
    }

    #[test]
    fn null_karsilastirmasi_en_kucuk() {
        assert_eq!(Deger::Null.karsilastir(&Deger::Tam(0)), Ordering::Less);
        assert_eq!(Deger::Tam(0).karsilastir(&Deger::Null), Ordering::Greater);
        assert_eq!(
            Deger::Blob(vec![]).karsilastir(&Deger::Metin(String::new())),
            Ordering::Greater
        );
    }

    #[test]
    fn tam_ve_gercel_karsilastirmasi() {
        assert_eq!(
            Deger::Tam(3).karsilastir(&Deger::Gercek(3.5)),
            Ordering::Less
        );
        assert_eq!(
            Deger::Gercek(3.5).karsilastir(&Deger::Tam(3)),
            Ordering::Greater
        );
        assert_eq!(
            Deger::Gercek(f64::NAN).karsilastir(&Deger::Gercek(1.0)),
            Ordering::Equal
        );
    }

    #[test]
    fn metin_karsilastirmasi_bayt_bazli() {
        // BINARY harmanlaması: 'B' (0x42) < 'a' (0x61), büyük/küçük harf gözetilmez.
        assert_eq!(
            Deger::Metin("B".into()).karsilastir(&Deger::Metin("a".into())),
            Ordering::Less
        );
        assert_eq!(
            Deger::Metin("a".into()).karsilastir(&Deger::Metin("B".into())),
            Ordering::Greater
        );
    }

    #[test]
    fn gosterim_kisaltmaz_ama_tip_yazar() {
        assert_eq!(Deger::Null.gosterim(), "NULL");
        assert_eq!(Deger::Blob(vec![0xde, 0xad]).gosterim(), "x'dead'");
        assert_eq!(Deger::Gercek(2.0).gosterim(), "2.0");
        assert_eq!(Deger::Gercek(2.25).gosterim(), "2.25");
    }

    #[test]
    fn json_temseri_dogru() {
        assert_eq!(
            Deger::Null.json_ser().unwrap_or(serde_json::Value::Null),
            serde_json::Value::Null
        );
        let b = Deger::Blob(vec![1, 255])
            .json_ser()
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(b["$blob"], serde_json::Value::String("01ff".into()));
    }

    #[test]
    fn sayisal_toplam_yalnizca_sayilarda() {
        assert_eq!(Deger::Tam(2).sayisal_toplam(), Some(2.0));
        assert_eq!(Deger::Metin("2".into()).sayisal_toplam(), None);
    }

    #[test]
    fn null_mu_ve_metin_yardimcilari() {
        assert!(Deger::Null.null_mu());
        assert!(!Deger::Tam(0).null_mu());
        assert_eq!(Deger::Metin("x".into()).metin(), Some("x"));
        assert_eq!(Deger::Tam(1).metin(), None);
    }
}
