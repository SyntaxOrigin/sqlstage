//! Şema modeli: `sqlite_master` okuması, kolon tipleri, birincil anahtar ve yabancı anahtar
//! ilişkileri.
//!
//! SQLite bir kolonun tipini depolamaz; tip yalnızca `CREATE TABLE` metninden bilinir.
//! Bu modül o metni ayrıştırır ve şemayı belirli bir yapıya dönüştürür.
//!
//! Desteklenmeyen yapılar (örneğin `WITHOUT ROWID` tablolar) açıkça hata verir; sessizce
//! yanlış şema üretilmez.

use std::path::Path;

use crate::deger::Deger;
use crate::hata::SahneHata;
use crate::sql::lexer::{tara, Jeton};
use crate::veritabani::Veritabani;

/// Bildirilen kolon tipinin sınıflandırılmış hâli.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KolonTuru {
    /// Tip bildirilmemiş.
    Bos,
    /// `INTEGER` ailesi.
    Tamsayi,
    /// `TEXT` / `VARCHAR` / `CHAR` / `CLOB` ailesi.
    Metin,
    /// `REAL` / `DOUBLE` / `FLOAT` ailesi.
    Gercek,
    /// `BLOB` ailesi.
    Blob,
    /// `NUMERIC` / `DECIMAL` ailesi.
    Sayisal,
    /// `DATE` / `DATETIME` / `TIMESTAMP` ailesi (tarih/saat öneki).
    ZamanDamgasi,
    /// `JSON` ailesi.
    Json,
    /// Başka bir tip adı (özgün yazımıyla saklanır).
    Diger(String),
}

impl KolonTuru {
    /// Bildirilen tip metninden sınıflandırma yapar.
    ///
    /// SQLite tip adına göre "tip yakınlığı" (affinity) uygular; burada yalnızca ilk
    /// kelimeye bakılır, sonraki kelimeler (örneğin `UNSIGNED BIG INT`) yok sayılır.
    pub fn belirle(bildirilen: &str) -> Self {
        // Uzunluk parantezi (VARCHAR(20)) ve çok kelimeli adlar (UNSIGNED BIG INT)
        // yalnızca ilk anlamlı kelimeye indirgenir.
        let ilk = bildirilen
            .split(['(', ' ', '\t', '\n'])
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        match ilk.as_str() {
            "" => Self::Bos,
            "INT" | "INTEGER" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "BIGINT" | "INT2"
            | "INT8" | "UNSIGNED" => Self::Tamsayi,
            "CHAR" | "CHARACTER" | "VARCHAR" | "NCHAR" | "NVARCHAR" | "TEXT" | "CLOB" => {
                Self::Metin
            }
            "REAL" | "DOUBLE" | "FLOAT" => Self::Gercek,
            "BLOB" => Self::Blob,
            "NUMERIC" | "DECIMAL" | "BOOLEAN" | "BOOL" => Self::Sayisal,
            "DATE" | "DATETIME" | "TIMESTAMP" | "TIME" => Self::ZamanDamgasi,
            "JSON" => Self::Json,
            _ => Self::Diger(bildirilen.to_ascii_uppercase()),
        }
    }

    /// Sınıflandırmanın okunabilir adını döndürür.
    pub fn ad(&self) -> &str {
        match self {
            Self::Bos => "(tip yok)",
            Self::Tamsayi => "INTEGER",
            Self::Metin => "TEXT",
            Self::Gercek => "REAL",
            Self::Blob => "BLOB",
            Self::Sayisal => "NUMERIC",
            Self::ZamanDamgasi => "DATETIME",
            Self::Json => "JSON",
            Self::Diger(ad) => ad.as_str(),
        }
    }
}

/// Tek bir kolonun şema bilgisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kolon {
    /// Kolon adı.
    pub ad: String,
    /// Sınıflandırılmış tip.
    pub tur: KolonTuru,
    /// `NOT NULL` bildirimi var mı.
    pub not_null: bool,
    /// Birincil anahtarın parçası mı.
    pub birincil_anahtar: bool,
    /// `DEFAULT` ifadesinin özgün metni (yoksa boş).
    pub varsayilan: Option<String>,
}

impl Kolon {
    /// Kolonun kısa etiketini üretir (sonuç başlıklarında kullanılır).
    pub fn etiket(&self) -> String {
        let mut parcalar = vec![self.ad.clone()];
        if self.not_null {
            parcalar.push("NOT NULL".to_string());
        }
        if self.birincil_anahtar {
            parcalar.push("PK".to_string());
        }
        parcalar.join(" ")
    }
}

/// Bir yabancı anahtar ilişkisi (ASCII diyagramda kullanılır).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YabanciAnahtar {
    /// Kaynak kolon adları.
    pub kolonlar: Vec<String>,
    /// Hedef tablo adı.
    pub hedef_tablo: String,
    /// Hedef kolon adları.
    pub hedef_kolonlar: Vec<String>,
}

/// Bir tablonun şema bilgisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tablo {
    /// Tablo adı.
    pub ad: String,
    /// Tablo b-tree'sinin kök sayfa numarası.
    pub kok_sayfa: u32,
    /// `sqlite_master.sql` alanındaki özgün `CREATE TABLE` metni.
    pub sql: String,
    /// Kolonlar, `CREATE TABLE` sırasına göre.
    pub kolonlar: Vec<Kolon>,
    /// Yabancı anahtar ilişkileri.
    pub yabancilar: Vec<YabanciAnahtar>,
    /// `sqlite_` önekiyle başlayan sistem tablosu mu.
    pub gizli: bool,
    /// Yaprak sayfalardaki hücre sayısı (satır sayısı).
    pub satir_sayisi: u64,
    /// Taramada ziyaret edilen b-tree sayfa sayısı.
    pub taranan_sayfa: u32,
    /// Tek kolonlu `INTEGER PRIMARY KEY` kolonunun dizini.
    ///
    /// SQLite bu kolonu kayıtta `NULL` olarak saklar, gerçek değeri hücrenin satır
    /// kimliğidir. Okuma sırasında bu dizindeki değer satır kimliğiyle değiştirilir.
    pub rowid_alan: Option<usize>,
}

impl Tablo {
    /// Kolon adına göre arama yapar (büyük/küçük harf duyarsız).
    pub fn kolon(&self, ad: &str) -> Option<&Kolon> {
        self.kolonlar.iter().find(|k| k.ad.eq_ignore_ascii_case(ad))
    }

    /// Kolon adından dizin üretir (`filtre` modülünün beklediği biçim).
    pub fn kolon_haritasi(&self) -> std::collections::HashMap<String, usize> {
        self.kolonlar
            .iter()
            .enumerate()
            .map(|(i, k)| (k.ad.to_ascii_lowercase(), i))
            .collect()
    }

    /// Birincil anahtar kolonlarının adlarını döndürür.
    pub fn birincil_anahtar(&self) -> Vec<&str> {
        self.kolonlar
            .iter()
            .filter(|k| k.birincil_anahtar)
            .map(|k| k.ad.as_str())
            .collect()
    }
}

/// Bir indeksin şema bilgisi (yalnızca listelenir, kullanılmaz).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indeks {
    /// İndeks adı.
    pub ad: String,
    /// İndeksin ait olduğu tablo.
    pub tablo: String,
    /// Kök sayfa numarası.
    pub kok_sayfa: u32,
}

/// Veritabanının tamamı.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sema {
    /// Kullanıcı ve sistem tabloları.
    pub tablolar: Vec<Tablo>,
    /// İndeksler.
    pub indeksler: Vec<Indeks>,
}

impl Sema {
    /// Adı verilen tabloyu büyük/küçük harf duyarsız arar.
    pub fn tablo(&self, ad: &str) -> Option<&Tablo> {
        self.tablolar.iter().find(|t| t.ad.eq_ignore_ascii_case(ad))
    }

    /// Kullanıcı tablolarını (sistem tabloları hariç) döndürür.
    pub fn kullanici_tablolari(&self) -> Vec<&Tablo> {
        self.tablolar.iter().filter(|t| !t.gizli).collect()
    }
}

/// Tek kolonlu `INTEGER PRIMARY KEY` kolonunun dizinini bulur (satır kimliği takma adı).
///
/// SQLite'ta `INTEGER PRIMARY KEY` kolonu bir *rowid alias*'tır: kayıtta `NULL` olarak
/// saklanır, gerçek değeri hücrenin satır kimliğidir. Bileşik (çok kolonlu) birincil
/// anahtar bu özelliğe sahip değildir.
pub fn rowid_alan_bul(kolonlar: &[Kolon]) -> Option<usize> {
    let adaylar: Vec<usize> = kolonlar
        .iter()
        .enumerate()
        .filter(|(_, k)| k.birincil_anahtar && k.tur == KolonTuru::Tamsayi)
        .map(|(i, _)| i)
        .collect();
    if adaylar.len() == 1
        && kolonlar
            .iter()
            .all(|k| !k.birincil_anahtar || k.tur == KolonTuru::Tamsayi)
    {
        adaylar.first().copied()
    } else {
        None
    }
}

/// Bir veritabanı dosyasının şemasını okur.
///
/// # Hatalar
///
/// `sqlite_master` bozuksa, bir tablonun `CREATE TABLE` metni yoksa veya
/// `WITHOUT ROWID` gibi desteklenmeyen bir yapı içeriyorsa hata döner.
pub fn sema_oku(db: &Veritabani) -> Result<Sema, SahneHata> {
    let mut sema = Sema::default();
    for satir in db.master_satirlari()? {
        let tur = satir
            .first()
            .and_then(Deger::metin)
            .unwrap_or("")
            .to_string();
        let ad = satir
            .get(1)
            .and_then(Deger::metin)
            .unwrap_or("")
            .to_string();
        let tablo_adi = satir
            .get(2)
            .and_then(Deger::metin)
            .unwrap_or(&ad)
            .to_string();
        let kok = match satir.get(3) {
            Some(Deger::Tam(k)) => *k as u32,
            _ => 0,
        };
        let sql = satir
            .get(4)
            .and_then(Deger::metin)
            .unwrap_or("")
            .to_string();
        match tur.as_str() {
            "table" => {
                let (kolonlar, yabancilar) = if sql.trim().is_empty() {
                    (Vec::new(), Vec::new())
                } else {
                    create_tablo_ayikla(&sql)?
                };
                let (satir_sayisi, taranan_sayfa, _) = db.tablo_yapisini_gez(kok)?;
                let rowid_alan = rowid_alan_bul(&kolonlar);
                sema.tablolar.push(Tablo {
                    gizli: ad.starts_with("sqlite_"),
                    ad,
                    kok_sayfa: kok,
                    sql,
                    kolonlar,
                    yabancilar,
                    satir_sayisi,
                    taranan_sayfa,
                    rowid_alan,
                });
            }
            "index" => sema.indeksler.push(Indeks {
                ad,
                tablo: tablo_adi,
                kok_sayfa: kok,
            }),
            _ => {}
        }
    }
    Ok(sema)
}

/// `CREATE TABLE` metninden kolon ve yabancı anahtar bilgisini çıkarır.
///
/// # Hatalar
///
/// Metin `CREATE TABLE` değilse, parantez dengesi bozuksa, `WITHOUT ROWID` içeriyorsa
/// veya tanınmayan bir kısıt varsa [`SahneHata::SemaHatasi`] döner.
pub fn create_tablo_ayikla(sql: &str) -> Result<(Vec<Kolon>, Vec<YabanciAnahtar>), SahneHata> {
    let tarama = tara(sql).map_err(|hata| SahneHata::SemaHatasi {
        mesaj: format!("CREATE TABLE metni okunamadı: {hata}"),
    })?;
    let jetonlar = tarama.jetonlar;
    let mut i = 0usize;
    let bos = |m: String| SahneHata::SemaHatasi { mesaj: m };

    anahtar(&jetonlar, &mut i, "CREATE")?;
    anahtar(&jetonlar, &mut i, "TABLE")?;
    if anahtar_var(&jetonlar, &mut i, "IF") {
        anahtar(&jetonlar, &mut i, "NOT")?;
        anahtar(&jetonlar, &mut i, "EXISTS")?;
    }
    kimlik(&jetonlar, &mut i, "tablo adı")?;
    islec(&jetonlar, &mut i, "(")?;

    let mut kolonlar: Vec<Kolon> = Vec::new();
    let mut yabancilar: Vec<YabanciAnahtar> = Vec::new();
    if islec_mi(&jetonlar, i, ")") {
        i += 1;
    } else {
        loop {
            if i >= jetonlar.len() {
                return Err(bos("CREATE TABLE parantez dengesi bozuk".to_string()));
            }
            let ilk = metin(&jetonlar, i);
            if ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"]
                .iter()
                .any(|k| ilk.eq_ignore_ascii_case(k))
            {
                parse_kisit(&jetonlar, &mut i, &mut kolonlar, &mut yabancilar)?;
            } else {
                parse_kolon(&jetonlar, &mut i, &mut kolonlar)?;
            }
            if islec_mi(&jetonlar, i, ",") {
                i += 1;
                continue;
            }
            if islec_mi(&jetonlar, i, ")") {
                i += 1;
                break;
            }
            return Err(bos(format!(
                "sütun listesinde beklenmeyen belirteç: {}",
                metin(&jetonlar, i)
            )));
        }
    }

    // Kapanış parantezinden sonra yalnızca `WITHOUT ROWID` ve `STRICT` gelebilir.
    let kalan: Vec<String> = jetonlar[i..].iter().map(jeton_metin).collect();
    if kalan.iter().any(|k| k.eq_ignore_ascii_case("WITHOUT")) {
        return Err(SahneHata::Desteklenmiyor {
            ozellik: "WITHOUT ROWID tablosu".to_string(),
            sebep: "WITHOUT ROWID tablolar indeks b-tree kullanır; desteklenmiyor",
        });
    }
    if kalan.iter().any(|k| k.eq_ignore_ascii_case("STRICT")) {
        return Err(SahneHata::Desteklenmiyor {
            ozellik: "STRICT tablo".to_string(),
            sebep: "STRICT tip zorlaması desteklenmiyor",
        });
    }
    if !kalan.is_empty() {
        return Err(bos(format!(
            "CREATE TABLE sonunda beklenmeyen ifade: {}",
            kalan.join(" ")
        )));
    }
    Ok((kolonlar, yabancilar))
}

/// Tek bir kolon tanımını ayrıştırır.
fn parse_kolon(
    jetonlar: &[Jeton],
    i: &mut usize,
    kolonlar: &mut Vec<Kolon>,
) -> Result<(), SahneHata> {
    let bos = |m: String| SahneHata::SemaHatasi { mesaj: m };
    let kolon_adi = kimlik(jetonlar, i, "kolon adı")?;
    if ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"]
        .iter()
        .any(|k| metin(jetonlar, *i).eq_ignore_ascii_case(k))
    {
        return Err(bos(format!(
            "kolon \"{kolon_adi}\" tanımı tip bildirmeden kısıt ile başlıyor"
        )));
    }
    // Tip adı: kısıt anahtar sözcüklerine, virgüle veya kapanış parantezine kadar.
    // Uzunluk belirteçleri (VARCHAR(20)) parantezli blok olarak tüketilir.
    let tip_basi = *i;
    while *i < jetonlar.len()
        && !islec_mi(jetonlar, *i, ",")
        && !islec_mi(jetonlar, *i, ")")
        && !is_kisit_anahtari(&metin(jetonlar, *i))
    {
        if islec_mi(jetonlar, *i, "(") {
            parantez_atla(jetonlar, i)?;
        } else {
            *i += 1;
        }
    }
    let tip_metni = jetonlar[tip_basi..*i]
        .iter()
        .map(jeton_metin)
        .collect::<Vec<_>>()
        .join(" ");
    // Tip adındaki uzunluk parantezlerini (VARCHAR(20)) at.
    let tip_sade = tip_metni.split('(').next().unwrap_or("").trim().to_string();

    let mut kolon = Kolon {
        ad: kolon_adi.clone(),
        tur: KolonTuru::belirle(&tip_sade),
        not_null: false,
        birincil_anahtar: false,
        varsayilan: None,
    };

    while *i < jetonlar.len() {
        let kelime = metin(jetonlar, *i);
        if kelime.eq_ignore_ascii_case("NOT") {
            anahtar(jetonlar, i, "NOT")?;
            anahtar(jetonlar, i, "NULL")?;
            kolon.not_null = true;
        } else if kelime.eq_ignore_ascii_case("PRIMARY") {
            anahtar(jetonlar, i, "PRIMARY")?;
            anahtar(jetonlar, i, "KEY")?;
            kolon.birincil_anahtar = true;
            // Çakışma çözümlemesi ve artan anahtar ifadeleri şema için gereksizdir.
            if anahtar_var(jetonlar, i, "ASC")
                || anahtar_var(jetonlar, i, "DESC")
                || anahtar_var(jetonlar, i, "AUTOINCREMENT")
            {
                continue;
            }
        } else if kelime.eq_ignore_ascii_case("UNIQUE") {
            anahtar(jetonlar, i, "UNIQUE")?;
        } else if kelime.eq_ignore_ascii_case("NULL") {
            anahtar(jetonlar, i, "NULL")?;
        } else if kelime.eq_ignore_ascii_case("DEFAULT") {
            anahtar(jetonlar, i, "DEFAULT")?;
            let bas = *i;
            parantez_icinde_mi(jetonlar, i);
            kolon.varsayilan = Some(
                jetonlar[bas..*i]
                    .iter()
                    .map(jeton_metin)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        } else if kelime.eq_ignore_ascii_case("COLLATE") {
            anahtar(jetonlar, i, "COLLATE")?;
            kimlik(jetonlar, i, "harmanlama adı")?;
        } else if kelime.eq_ignore_ascii_case("CHECK") {
            anahtar(jetonlar, i, "CHECK")?;
            parantez_atla(jetonlar, i)?;
        } else if kelime.eq_ignore_ascii_case("REFERENCES") {
            anahtar(jetonlar, i, "REFERENCES")?;
            let _ = yabanci_oku(jetonlar, i)?;
        } else if kelime.eq_ignore_ascii_case("GENERATED") {
            return Err(SahneHata::Desteklenmiyor {
                ozellik: "GENERATED ... AS kolonu".to_string(),
                sebep: "üretilmiş kolonlar desteklenmiyor",
            });
        } else {
            break;
        }
    }
    if kolonlar
        .iter()
        .any(|k| k.ad.eq_ignore_ascii_case(&kolon_adi))
    {
        return Err(bos(format!("yinelenen kolon adı: {kolon_adi}")));
    }
    kolonlar.push(kolon);
    Ok(())
}

/// Tablo düzeyi kısıtları ayrıştırır.
fn parse_kisit(
    jetonlar: &[Jeton],
    i: &mut usize,
    kolonlar: &mut [Kolon],
    yabancilar: &mut Vec<YabanciAnahtar>,
) -> Result<(), SahneHata> {
    let bos = |m: String| SahneHata::SemaHatasi { mesaj: m };
    if anahtar_var(jetonlar, i, "CONSTRAINT") {
        kimlik(jetonlar, i, "kısıt adı")?;
    }
    let kelime = metin(jetonlar, *i);
    if kelime.eq_ignore_ascii_case("PRIMARY") {
        anahtar(jetonlar, i, "PRIMARY")?;
        anahtar(jetonlar, i, "KEY")?;
        let kolonlar_listesi = kolon_listesi(jetonlar, i)?;
        for ad in &kolonlar_listesi {
            let bulundu = kolonlar
                .iter_mut()
                .find(|k| k.ad.eq_ignore_ascii_case(ad))
                .ok_or_else(|| bos(format!("PRIMARY KEY bilinmeyen kolonu içeriyor: {ad}")))?;
            bulundu.birincil_anahtar = true;
        }
        cakisma_bitis(jetonlar, i);
        return Ok(());
    }
    if kelime.eq_ignore_ascii_case("UNIQUE") {
        anahtar(jetonlar, i, "UNIQUE")?;
        let _ = kolon_listesi(jetonlar, i)?;
        cakisma_bitis(jetonlar, i);
        return Ok(());
    }
    if kelime.eq_ignore_ascii_case("CHECK") {
        anahtar(jetonlar, i, "CHECK")?;
        parantez_atla(jetonlar, i)?;
        return Ok(());
    }
    if kelime.eq_ignore_ascii_case("FOREIGN") {
        anahtar(jetonlar, i, "FOREIGN")?;
        anahtar(jetonlar, i, "KEY")?;
        let kaynak = kolon_listesi(jetonlar, i)?;
        anahtar(jetonlar, i, "REFERENCES")?;
        let (hedef_tablo, hedef_kolonlar) = yabanci_oku(jetonlar, i)?;
        yabancilar.push(YabanciAnahtar {
            kolonlar: kaynak,
            hedef_tablo,
            hedef_kolonlar,
        });
        return Ok(());
    }
    Err(bos(format!("tanınmayan tablo kısıtı: {kelime}")))
}

/// `REFERENCES tablo (kolonlar)` bölümünü okur.
fn yabanci_oku(jetonlar: &[Jeton], i: &mut usize) -> Result<(String, Vec<String>), SahneHata> {
    let hedef = kimlik(jetonlar, i, "yabancı anahtar hedef tablosu")?;
    let mut kolonlar = Vec::new();
    if islec_mi(jetonlar, *i, "(") {
        kolonlar = kolon_listesi(jetonlar, i)?;
    }
    // ON DELETE / ON UPDATE / DEFERRABLE gibi ek yan cümleler şema için gereksizdir.
    while *i < jetonlar.len() && !islec_mi(jetonlar, *i, ",") && !islec_mi(jetonlar, *i, ")") {
        *i += 1;
    }
    Ok((hedef, kolonlar))
}

/// Parantez içindeki virgüllü kimlik listesini okur.
fn kolon_listesi(jetonlar: &[Jeton], i: &mut usize) -> Result<Vec<String>, SahneHata> {
    islec(jetonlar, i, "(")?;
    let mut liste = Vec::new();
    if islec_mi(jetonlar, *i, ")") {
        *i += 1;
        return Ok(liste);
    }
    loop {
        liste.push(kimlik(jetonlar, i, "kolon adı")?);
        if islec_mi(jetonlar, *i, ",") {
            *i += 1;
            continue;
        }
        islec(jetonlar, i, ")")?;
        break;
    }
    Ok(liste)
}

/// Dengeli parantez bloğunu atlar.
fn parantez_atla(jetonlar: &[Jeton], i: &mut usize) -> Result<(), SahneHata> {
    islec(jetonlar, i, "(")?;
    let mut derinlik = 1usize;
    while *i < jetonlar.len() && derinlik > 0 {
        if islec_mi(jetonlar, *i, "(") {
            derinlik += 1;
        } else if islec_mi(jetonlar, *i, ")") {
            derinlik -= 1;
        }
        *i += 1;
    }
    if derinlik != 0 {
        return Err(SahneHata::SemaHatasi {
            mesaj: "kapanmamış parantez".to_string(),
        });
    }
    Ok(())
}

/// `DEFAULT` ifadesinin parantezli kısmını varsa atlar.
fn parantez_icinde_mi(jetonlar: &[Jeton], i: &mut usize) {
    if islec_mi(jetonlar, *i, "(") {
        let _ = parantez_atla(jetonlar, i);
    } else {
        *i += 1;
    }
}

/// `ON CONFLICT` / `DEFERRABLE` gibi ek ifadeleri atlar.
fn cakisma_bitis(jetonlar: &[Jeton], i: &mut usize) {
    if anahtar_var(jetonlar, i, "ON") {
        let _ = anahtar_var(jetonlar, i, "CONFLICT");
        let _ = anahtar_var(jetonlar, i, "ROLLBACK")
            || anahtar_var(jetonlar, i, "ABORT")
            || anahtar_var(jetonlar, i, "FAIL")
            || anahtar_var(jetonlar, i, "IGNORE")
            || anahtar_var(jetonlar, i, "REPLACE");
    }
}

fn is_kisit_anahtari(metin: &str) -> bool {
    [
        "NOT",
        "NULL",
        "PRIMARY",
        "UNIQUE",
        "CHECK",
        "DEFAULT",
        "COLLATE",
        "REFERENCES",
        "CONSTRAINT",
        "GENERATED",
        "AS",
    ]
    .iter()
    .any(|k| metin.eq_ignore_ascii_case(k))
}

/// Bir belirteci metne çevirir (hata mesajlarında ve birleştirmede kullanılır).
fn jeton_metin(j: &Jeton) -> String {
    match j {
        Jeton::Kimlik(k) => k.clone(),
        Jeton::TamSayi(i) => i.to_string(),
        Jeton::KayanSayi(f) => f.to_string(),
        Jeton::Metin(s) => format!("'{s}'"),
        Jeton::Islec(i) => (*i).to_string(),
    }
}

/// `*i` konumundaki belirteci metne çevirir (yoksa boş string).
fn metin(jetonlar: &[Jeton], i: usize) -> String {
    jetonlar.get(i).map(jeton_metin).unwrap_or_default()
}

/// `*i` konumunda beklenen anahtar sözcük varsa tüketir.
fn anahtar(jetonlar: &[Jeton], i: &mut usize, beklenen: &str) -> Result<(), SahneHata> {
    if metin(jetonlar, *i).eq_ignore_ascii_case(beklenen) {
        *i += 1;
        return Ok(());
    }
    Err(SahneHata::SemaHatasi {
        mesaj: format!(
            "CREATE TABLE içinde {beklenen} bekleniyordu, bulunan: {}",
            metin(jetonlar, *i)
        ),
    })
}

/// `*i` konumunda beklenen işleç varsa tüketir.
fn islec(jetonlar: &[Jeton], i: &mut usize, beklenen: &str) -> Result<(), SahneHata> {
    if islec_mi(jetonlar, *i, beklenen) {
        *i += 1;
        return Ok(());
    }
    Err(SahneHata::SemaHatasi {
        mesaj: format!(
            "CREATE TABLE içinde {beklenen} bekleniyordu, bulunan: {}",
            metin(jetonlar, *i)
        ),
    })
}

/// `*i` konumunda anahtar sözcük varsa tüketir ve `true` döndürür.
fn anahtar_var(jetonlar: &[Jeton], i: &mut usize, beklenen: &str) -> bool {
    if metin(jetonlar, *i).eq_ignore_ascii_case(beklenen) {
        *i += 1;
        return true;
    }
    false
}

fn islec_mi(jetonlar: &[Jeton], i: usize, beklenen: &str) -> bool {
    matches!(jetonlar.get(i), Some(Jeton::Islec(x)) if *x == beklenen)
}

/// Kimlik bekler, tırnaklı kimlikleri de kabul eder.
fn kimlik(jetonlar: &[Jeton], i: &mut usize, beklenen: &str) -> Result<String, SahneHata> {
    match jetonlar.get(*i) {
        Some(Jeton::Kimlik(k)) => {
            let ad = k.clone();
            *i += 1;
            Ok(ad)
        }
        _ => Err(SahneHata::SemaHatasi {
            mesaj: format!("{beklenen} bekleniyordu, bulunan: {}", metin(jetonlar, *i)),
        }),
    }
}

/// Bir dosyanın şemasını okuyan kolay giriş noktası (CLI tarafından kullanılır).
pub fn dosya_semasini_oku(yol: &Path) -> Result<Sema, SahneHata> {
    let db = Veritabani::ac(yol)?;
    sema_oku(&db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yazici::{veritabani_goruntusu_sayfa_boyutu, TabloYazimi, TEST_SAYFA_BOYUTU};

    #[test]
    fn kolon_turu_siniflandirmasi() {
        assert_eq!(KolonTuru::belirle("INTEGER"), KolonTuru::Tamsayi);
        assert_eq!(KolonTuru::belirle("VARCHAR(20)"), KolonTuru::Metin);
        assert_eq!(KolonTuru::belirle("DOUBLE PRECISION"), KolonTuru::Gercek);
        assert_eq!(KolonTuru::belirle("BLOB"), KolonTuru::Blob);
        assert_eq!(KolonTuru::belirle("NUMERIC(10,2)"), KolonTuru::Sayisal);
        assert_eq!(KolonTuru::belirle("TIMESTAMP"), KolonTuru::ZamanDamgasi);
        assert_eq!(KolonTuru::belirle("JSON"), KolonTuru::Json);
        assert_eq!(KolonTuru::belirle(""), KolonTuru::Bos);
        assert_eq!(
            KolonTuru::belirle("geopolygon"),
            KolonTuru::Diger("GEOPOLYGON".into())
        );
        assert_eq!(KolonTuru::Tamsayi.ad(), "INTEGER");
    }

    #[test]
    fn basit_create_table_ayristirilir() {
        let (kolonlar, yabancilar) =
            match create_tablo_ayikla("CREATE TABLE kisi(id INTEGER, ad TEXT, yas INTEGER)") {
                Ok(v) => v,
                Err(hata) => panic!("beklenmeyen hata: {hata}"),
            };
        assert_eq!(kolonlar.len(), 3);
        assert_eq!(kolonlar[0].ad, "id");
        assert_eq!(kolonlar[1].tur, KolonTuru::Metin);
        assert!(yabancilar.is_empty());
    }

    #[test]
    fn kisitlar_okunur() {
        let (kolonlar, _) = match create_tablo_ayikla(
            "CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, ad TEXT NOT NULL DEFAULT 'x', yas INT)",
        ) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(kolonlar[0].birincil_anahtar);
        assert!(kolonlar[1].not_null);
        assert_eq!(kolonlar[1].varsayilan.as_deref(), Some("'x'"));
        assert!(!kolonlar[2].not_null);
    }

    #[test]
    fn tablo_duzeyi_birincil_anahtar_okunur() {
        let (kolonlar, _) =
            match create_tablo_ayikla("CREATE TABLE t(a INT, b INT, PRIMARY KEY (a, b))") {
                Ok(v) => v,
                Err(hata) => panic!("beklenmeyen hata: {hata}"),
            };
        assert!(kolonlar.iter().all(|k| k.birincil_anahtar));
    }

    #[test]
    fn yabanci_anahtar_okunur() {
        let (_, yabancilar) = match create_tablo_ayikla(
            "CREATE TABLE siparis(id INTEGER PRIMARY KEY, musteri_id INTEGER, FOREIGN KEY (musteri_id) REFERENCES musteriler(id))",
        ) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(yabancilar.len(), 1);
        assert_eq!(yabancilar[0].kolonlar, vec!["musteri_id"]);
        assert_eq!(yabancilar[0].hedef_tablo, "musteriler");
        assert_eq!(yabancilar[0].hedef_kolonlar, vec!["id"]);
    }

    #[test]
    fn kolon_duzeyi_references_okunur() {
        let (_, yabancilar) =
            match create_tablo_ayikla("CREATE TABLE t(a INT REFERENCES u(id) ON DELETE CASCADE)") {
                Ok(v) => v,
                Err(hata) => panic!("beklenmeyen hata: {hata}"),
            };
        // Kolon düzeyindeki REFERENCES yabancı anahtar listesine eklenmez (şema yeterli).
        assert!(yabancilar.is_empty());
    }

    #[test]
    fn uzun_tip_adlari_okunur() {
        let (kolonlar, _) =
            match create_tablo_ayikla("CREATE TABLE t(a UNSIGNED BIG INT, b VARCHAR(50) NOT NULL)")
            {
                Ok(v) => v,
                Err(hata) => panic!("beklenmeyen hata: {hata}"),
            };
        assert_eq!(kolonlar[0].tur, KolonTuru::Tamsayi);
        assert_eq!(kolonlar[1].tur, KolonTuru::Metin);
        assert!(kolonlar[1].not_null);
    }

    #[test]
    fn without_rowid_reddedilir() {
        let sonuc = create_tablo_ayikla("CREATE TABLE t(a TEXT PRIMARY KEY) WITHOUT ROWID");
        assert!(matches!(sonuc, Err(SahneHata::Desteklenmiyor { .. })));
    }

    #[test]
    fn strict_reddedilir() {
        let sonuc = create_tablo_ayikla("CREATE TABLE t(a INT) STRICT");
        assert!(matches!(sonuc, Err(SahneHata::Desteklenmiyor { .. })));
    }

    #[test]
    fn yinelenen_kolon_reddedilir() {
        let sonuc = create_tablo_ayikla("CREATE TABLE t(a INT, a TEXT)");
        assert!(sonuc.is_err());
    }

    #[test]
    fn kapanmamis_parantez_reddedilir() {
        assert!(create_tablo_ayikla("CREATE TABLE t(a INT").is_err());
    }

    #[test]
    fn create_table_disi_reddedilir() {
        assert!(create_tablo_ayikla("DROP TABLE t").is_err());
    }

    #[test]
    fn tam_sema_dosyadan_okunur() {
        let goruntu = match veritabani_goruntusu_sayfa_boyutu(
            &[
                TabloYazimi {
                    ad: "musteriler".into(),
                    sql: "CREATE TABLE musteriler(id INTEGER PRIMARY KEY, ad TEXT NOT NULL)".into(),
                    satirlar: vec![(1, vec![Deger::Tam(1), Deger::Metin("Ali".into())])],
                },
                TabloYazimi {
                    ad: "siparisler".into(),
                    sql: "CREATE TABLE siparisler(id INTEGER PRIMARY KEY, musteri_id INTEGER, FOREIGN KEY (musteri_id) REFERENCES musteriler(id))".into(),
                    satirlar: vec![(1, vec![Deger::Tam(1), Deger::Tam(1)])],
                },
            ],
            TEST_SAYFA_BOYUTU,
        ) {
            Ok(g) => g,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let dizin = std::env::temp_dir().join(format!("sqlstage-sema-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dizin);
        let yol = dizin.join("s.db");
        std::fs::write(&yol, goruntu).unwrap_or_default();
        let db = match Veritabani::ac(&yol) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        let sema = match sema_oku(&db) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(sema.tablolar.len(), 2);
        let musteriler = sema
            .tablo("musteriler")
            .unwrap_or_else(|| panic!("tablo yok"));
        assert_eq!(musteriler.birincil_anahtar(), vec!["id"]);
        assert!(
            musteriler
                .kolon("ad")
                .unwrap_or_else(|| panic!("kolon yok"))
                .not_null
        );
        assert_eq!(musteriler.satir_sayisi, 1);
        let siparisler = sema
            .tablo("siparisler")
            .unwrap_or_else(|| panic!("tablo yok"));
        assert_eq!(siparisler.yabancilar.len(), 1);
        assert_eq!(sema.kullanici_tablolari().len(), 2);
        let _ = std::fs::remove_dir_all(&dizin);
    }

    #[test]
    fn rowid_takma_adi_tam_sayi_birincil_anahtardir() {
        let tek = vec![
            Kolon {
                ad: "id".into(),
                tur: KolonTuru::Tamsayi,
                not_null: false,
                birincil_anahtar: true,
                varsayilan: None,
            },
            Kolon {
                ad: "ad".into(),
                tur: KolonTuru::Metin,
                not_null: false,
                birincil_anahtar: false,
                varsayilan: None,
            },
        ];
        assert_eq!(rowid_alan_bul(&tek), Some(0));

        // Metin birincil anahtar rowid alias değildir.
        let metin_pk = vec![Kolon {
            ad: "ad".into(),
            tur: KolonTuru::Metin,
            not_null: false,
            birincil_anahtar: true,
            varsayilan: None,
        }];
        assert_eq!(rowid_alan_bul(&metin_pk), None);

        // Bileşik birincil anahtar da rowid alias değildir.
        let bilesik = vec![
            Kolon {
                ad: "a".into(),
                tur: KolonTuru::Tamsayi,
                not_null: false,
                birincil_anahtar: true,
                varsayilan: None,
            },
            Kolon {
                ad: "b".into(),
                tur: KolonTuru::Tamsayi,
                not_null: false,
                birincil_anahtar: true,
                varsayilan: None,
            },
        ];
        assert_eq!(rowid_alan_bul(&bilesik), None);
    }

    #[test]
    fn etiket_ve_harita_ayristirmasi() {
        let tablo = Tablo {
            ad: "t".into(),
            kok_sayfa: 2,
            sql: String::new(),
            kolonlar: vec![Kolon {
                ad: "id".into(),
                tur: KolonTuru::Tamsayi,
                not_null: true,
                birincil_anahtar: true,
                varsayilan: None,
            }],
            yabancilar: Vec::new(),
            gizli: false,
            satir_sayisi: 0,
            taranan_sayfa: 1,
            rowid_alan: Some(0),
        };
        assert_eq!(tablo.kolonlar[0].etiket(), "id NOT NULL PK");
        let harita = tablo.kolon_haritasi();
        assert_eq!(harita.get("id").copied(), Some(0));
        assert!(tablo.kolon("ID").is_some());
        assert!(tablo.kolon("yok").is_none());
    }
}
