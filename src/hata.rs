//! SQLStage hata tipi.
//!
//! `thiserror` bağımlılığı yasak olduğu için `Display` ve `std::error::Error`
//! uygulamaları elle yazıldı. Kullanıcı girdisinden doğan her koşul `Result` ile döner;
//! üretim kodunda `unwrap`, `expect` ve `panic!` yoktur.

use std::fmt;

/// SQLStage'in tüm hata koşullarını taşıyan enum.
///
/// `#[non_exhaustive]` değildir: eşleme yapan tek yer modülün kendisidir ve yeni
/// varyant eklemek kaynak uyumlu kırılma olmamalıdır.
#[derive(Debug)]
pub enum SahneHata {
    /// Dosya okunamadı veya okuma sırasında hata oluştu.
    Io {
        /// İşlem yapılan yol.
        yol: String,
        /// Altındaki `std::io::Error`.
        kaynak: std::io::Error,
    },
    /// Dosya beklenen minimum uzunlukta değil (en az 100 bayt + 1. sayfa).
    DosyaCokKisa {
        /// Gerçek bayt sayısı.
        bayt: usize,
    },
    /// Başlıktaki sihirli sabit `SQLite format 3\0` değil.
    MagicHatali {
        /// Okunan ilk baytların metin karşılığı.
        bulunan: String,
    },
    /// Başlık alanı geçersiz bir değer taşıyor.
    GecersizBaslik {
        /// Alan adı (hata mesajında görünür).
        alan: &'static str,
        /// Okunan değer.
        deger: u64,
        /// Açıklama.
        sebep: &'static str,
    },
    /// Sayfa numarası dosya sınırları dışında.
    SayfaDisinda {
        /// İstenen sayfa numarası (1 tabanlı).
        istenen: u32,
        /// Dosyadaki toplam sayfa sayısı.
        mevcut: u32,
    },
    /// Bir sayfa b-tree düzeni olarak çözülemedi.
    BozukSayfa {
        /// Sayfa numarası.
        sayfa: u32,
        /// Hatanın ayrıntısı.
        ayrinti: String,
    },
    /// Tablo b-tree'si geçersiz (silinmiş sayfa, döngü, derinlik sınırı).
    BozukAgac {
        /// Tablo adı.
        tablo: String,
        /// Hatanın ayrıntısı.
        ayrinti: String,
    },
    /// Kayıt (serial type) ayrıştırılamadı.
    BozukKayit {
        /// Hatanın ayrıntısı.
        ayrinti: String,
    },
    /// Dosya UTF-8/UTF-16 dışı bir kodlama bildiriyor.
    KodlamaDesteklenmiyor {
        /// Başlıkta bildirilen kodlama numarası.
        kod: u64,
    },
    /// Varışma (overflow) zinciri dosya dışına çıkıyor veya döngüye giriyor.
    BozukVarisma {
        /// Hatanın ayrıntısı.
        ayrinti: String,
    },
    /// Şemada istenen tablo yok.
    TabloYok {
        /// Aranan tablo adı.
        ad: String,
        /// Veritabanındaki mevcut tablo adları.
        mevcut: Vec<String>,
    },
    /// Tablo var ama istenen kolon yok.
    KolonYok {
        /// Tablo adı.
        tablo: String,
        /// Aranan kolon adı.
        kolon: String,
    },
    /// SQL metni ayrıştırılamadı.
    SorguHatasi {
        /// Hata açıklaması.
        mesaj: String,
        /// Hatanın yaklaşık karakter konumu (bayt indeksi).
        konum: usize,
    },
    /// SQL geçerli ama bu alt kümenin dışında (JOIN, alt sorgu, toplam fonksiyon...).
    Desteklenmiyor {
        /// Desteklenmeyen yapı.
        ozellik: String,
        /// Neden desteklenmediğinin kısa açıklaması.
        sebep: &'static str,
    },
    /// Veritabanını değiştiren komut reddedildi (salt okunur politika).
    YazmaReddi {
        /// Reddedilen komutun ilk kelimesi.
        komut: String,
    },
    /// Şema oluşturma/değiştirme ifadesi okunamadı.
    SemaHatasi {
        /// Hata açıklaması.
        mesaj: String,
    },
    /// JSON üretimi başarısız.
    Json(String),
}

impl fmt::Display for SahneHata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { yol, kaynak } => write!(f, "dosya okunamadı ({yol}): {kaynak}"),
            Self::DosyaCokKisa { bayt } => write!(
                f,
                "dosya çok kısa: {bayt} bayt (en az 100 bayt başlık + sayfa gerekir)"
            ),
            Self::MagicHatali { bulunan } => write!(
                f,
                "geçersiz veritabanı: başlık sihirli sabiti \"SQLite format 3\\0\" değil, okunan: {bulunan:?}"
            ),
            Self::GecersizBaslik { alan, deger, sebep } => {
                write!(f, "geçersiz başlık alanı {alan}={deger}: {sebep}")
            }
            Self::SayfaDisinda { istenen, mevcut } => write!(
                f,
                "sayfa {istenen} dosya sınırı dışında (dosyada {mevcut} sayfa var)"
            ),
            Self::BozukSayfa { sayfa, ayrinti } => write!(f, "bozuk sayfa {sayfa}: {ayrinti}"),
            Self::BozukAgac { tablo, ayrinti } => {
                write!(f, "tablo \"{tablo}\" b-tree ağacı bozuk: {ayrinti}")
            }
            Self::BozukKayit { ayrinti } => write!(f, "bozuk kayıt: {ayrinti}"),
            Self::KodlamaDesteklenmiyor { kod } => write!(
                f,
                "metin kodlaması {kod} desteklenmiyor (yalnızca UTF-8/UTF-16LE/UTF-16BE)"
            ),
            Self::BozukVarisma { ayrinti } => write!(f, "bozuk varışma zinciri: {ayrinti}"),
            Self::TabloYok { ad, mevcut } => write!(
                f,
                "tablo bulunamadı: \"{ad}\" (mevcut: {})",
                if mevcut.is_empty() {
                    "yok".to_string()
                } else {
                    mevcut.join(", ")
                }
            ),
            Self::KolonYok { tablo, kolon } => {
                write!(f, "kolon bulunamadı: {tablo}.{kolon}")
            }
            Self::SorguHatasi { mesaj, konum } => {
                write!(f, "SQL ayrıştırma hatası (konum {konum}): {mesaj}")
            }
            Self::Desteklenmiyor { ozellik, sebep } => {
                write!(f, "desteklenmeyen SQL yapısı: {ozellik} — {sebep}")
            }
            Self::YazmaReddi { komut } => write!(
                f,
                "yazma komutu reddedildi: \"{komut}\" — SQLStage veritabanı dosyasını salt okunur açar"
            ),
            Self::SemaHatasi { mesaj } => write!(f, "şema okunamadı: {mesaj}"),
            Self::Json(mesaj) => write!(f, "JSON üretilemedi: {mesaj}"),
        }
    }
}

impl std::error::Error for SahneHata {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { kaynak, .. } => Some(kaynak),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SahneHata {
    fn from(kaynak: std::io::Error) -> Self {
        Self::Io {
            yol: "<io>".to_string(),
            kaynak,
        }
    }
}
