//! SQL alt kümesinin soyut sözdizim ağacı (AST).
//!
//! Bu türler ayrıştırıcının çıktısıdır; yürütücü ve plan üreticisi yalnızca bunları okur.
//! Desteklenmeyen yapılar (JOIN, alt sorgu, toplam fonksiyon...) AST'de **yer almaz**;
//! ayrıştırıcı bunları daha önce reddeder.

/// Karşılaştırma işleçleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KarsilastirmaIsleci {
    /// `=`
    Esit,
    /// `!=` veya `<>`
    EsitDegil,
    /// `<`
    Kucuk,
    /// `>`
    Buyuk,
    /// `<=`
    KucukEsit,
    /// `>=`
    BuyukEsit,
}

impl KarsilastirmaIsleci {
    /// İşlecin okunabilir metnini döndürür.
    pub fn metin(&self) -> &'static str {
        match self {
            Self::Esit => "=",
            Self::EsitDegil => "!=",
            Self::Kucuk => "<",
            Self::Buyuk => ">",
            Self::KucukEsit => "<=",
            Self::BuyukEsit => ">=",
        }
    }
}

/// `WHERE` bölümünde bir kolonla karşılaştırılan sabit.
#[derive(Debug, Clone, PartialEq)]
pub enum Sabit {
    /// Tamsayı sabiti.
    Tam(i64),
    /// Gerçek sayı sabiti.
    Kayan(f64),
    /// Metin sabiti.
    Metin(String),
    /// `NULL` sabiti (`= NULL` her zaman boş sonuç verir; `IS NULL` kullanılmalıdır).
    Null,
}

impl Sabit {
    /// Sabiti hücre değerine çevirir.
    pub fn deger(&self) -> crate::deger::Deger {
        match self {
            Self::Tam(i) => crate::deger::Deger::Tam(*i),
            Self::Kayan(f) => crate::deger::Deger::Gercek(*f),
            Self::Metin(s) => crate::deger::Deger::Metin(s.clone()),
            Self::Null => crate::deger::Deger::Null,
        }
    }
}

/// `WHERE` koşulu ifadesi.
#[derive(Debug, Clone, PartialEq)]
pub enum Ifade {
    /// `kolon <işleç> sabit`
    Karsilastirma {
        /// Sol taraftaki kolon adı.
        sutun: String,
        /// Karşılaştırma işleci.
        islec: KarsilastirmaIsleci,
        /// Sağ taraftaki sabit.
        sabit: Sabit,
    },
    /// `kolon LIKE 'desen'` (veya `NOT LIKE`)
    Benzer {
        /// Kolon adı.
        sutun: String,
        /// Desen metni (`%` ve `_` jokerleri içerir).
        desen: String,
        /// `NOT LIKE` kullanıldıysa `true`.
        degil: bool,
    },
    /// `kolon IS NULL` / `kolon IS NOT NULL`
    NullMu {
        /// Kolon adı.
        sutun: String,
        /// Beklenen değer `NULL` ise `true`.
        bekliyor: bool,
    },
    /// `a AND b AND ...`
    Ve(Vec<Ifade>),
    /// `a OR b OR ...`
    Ya(Vec<Ifade>),
    /// `NOT ifade`
    Tumu {
        /// Sarılan ifade.
        ic: Box<Ifade>,
        /// Her zaman `true`.
        degil: bool,
    },
}

impl Ifade {
    /// İfadenin karşılaştırma sayısını (derinlik bilgisi olmadan) döndürür.
    ///
    /// Sorgu plan açıklamasında "kaç koşul uygulandı" bilgisini üretmek için kullanılır.
    pub fn karsilastirma_sayisi(&self) -> usize {
        match self {
            Self::Karsilastirma { .. } | Self::Benzer { .. } | Self::NullMu { .. } => 1,
            Self::Ve(hepler) | Self::Ya(hepler) => {
                hepler.iter().map(Self::karsilastirma_sayisi).sum()
            }
            Self::Tumu { ic, .. } => ic.karsilastirma_sayisi(),
        }
    }
}

/// `SELECT` kolon listesi.
#[derive(Debug, Clone, PartialEq)]
pub enum Secim {
    /// `SELECT *`
    Yildiz,
    /// Açık kolon listesi.
    Sutunlar(Vec<Projeksiyon>),
}

impl Secim {
    /// Seçimdeki kolon sayısını döndürür (`*` için 0).
    pub fn acik_sayi(&self) -> usize {
        match self {
            Self::Yildiz => 0,
            Self::Sutunlar(l) => l.len(),
        }
    }
}

/// Tek bir seçilen kolon (isteğe bağlı takma adlı).
#[derive(Debug, Clone, PartialEq)]
pub struct Projeksiyon {
    /// Kaynak kolon adı.
    pub sutun: String,
    /// `AS` ile verilen takma ad.
    pub takma_ad: Option<String>,
}

impl Projeksiyon {
    /// Sonuç ızgarasında görünecek başlığı döndürür.
    pub fn baslik(&self) -> &str {
        self.takma_ad.as_deref().unwrap_or(self.sutun.as_str())
    }
}

/// `ORDER BY` maddesi.
#[derive(Debug, Clone, PartialEq)]
pub struct Siralama {
    /// Sıralanacak kolon adı.
    pub sutun: String,
    /// `DESC` kullanıldıysa `true`.
    pub azalan: bool,
}

/// Ayrıştırılmış bir `SELECT` sorgusu.
#[derive(Debug, Clone, PartialEq)]
pub struct Sorgu {
    /// Kullanıcının yazdığı özgün metin (plan açıklamasında gösterilir).
    pub metin: String,
    /// Kaynak tablonun adı.
    pub tablo: String,
    /// Kolon seçimi.
    pub secim: Secim,
    /// `WHERE` koşulu (yoksa `None`).
    pub filtre: Option<Ifade>,
    /// `ORDER BY` maddeleri.
    pub siralama: Vec<Siralama>,
    /// `LIMIT` değeri.
    pub limit: Option<usize>,
    /// `OFFSET` değeri.
    pub ofset: Option<usize>,
}

impl Sorgu {
    /// Sorgunun tek satırlı, okunabilir özetini döndürür.
    pub fn ozet(&self) -> String {
        let mut parcalar: Vec<String> = Vec::new();
        if let Some(limit) = self.limit {
            parcalar.push(format!("LIMIT {limit}"));
        }
        if let Some(ofset) = self.ofset {
            parcalar.push(format!("OFFSET {ofset}"));
        }
        parcalar.join(" ")
    }
}
