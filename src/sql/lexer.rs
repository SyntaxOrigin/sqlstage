//! SQL sözcük tarağıcısı (lexer).
//!
//! Yalnızca SQL alt kümemizin ihtiyaç duyduğu belirteçleri üretir: kimlik, tamsayı,
//! kayan sayı, metin ve işleçler. Değişkenler, fonksiyon adları ve yorumlar ayrıştırma
//! katmanında tanınır.

use crate::hata::SahneHata;

/// Üretilen tek bir sözcük.
#[derive(Debug, Clone, PartialEq)]
pub enum Jeton {
    /// Kimlik ya da anahtar sözcük (büyük/küçük harf korunur, karşılaştırma büyük harfle).
    Kimlik(String),
    /// Tamsayı sabiti.
    TamSayi(i64),
    /// Kayan nokta sabiti.
    KayanSayi(f64),
    /// Tek tırnaklı metin sabiti (çift tırnak kaçışı çözülmüş hâlde).
    Metin(String),
    /// Sözcüksel işleç veya ayraç.
    Islec(&'static str),
}

/// Sözcük taramasının sonucu: belirteçler ve kapanış konumu.
#[derive(Debug, Clone)]
pub struct Tarama {
    /// Üretilen belirteçler.
    pub jetonlar: Vec<Jeton>,
    /// Son belirteçten sonraki konum (hata mesajlarında kullanılır).
    pub son_konum: usize,
}

impl Tarama {
    /// İlk belirteci büyük harfe çevirilmiş hâlde döndürür (`SELECT` → `SELECT`).
    pub fn ilk_anahtar(&self) -> Option<String> {
        match self.jetonlar.first() {
            Some(Jeton::Kimlik(k)) => Some(k.to_ascii_uppercase()),
            _ => None,
        }
    }
}

/// SQL metnini belirteçlere ayırır.
///
/// Tanınan işleçler: `(` `)` `,` `.` `;` `*` `=` `==` `!=` `<>` `<` `>` `<=` `>=`.
/// `--` ile satır sonuna kadar ve `/* ... */` arasındaki metinler yorum sayılır.
///
/// # Hatalar
///
/// Kapanmamış metin sabiti, geçersiz sayı veya tanınmayan karakter durumunda
/// [`SahneHata::SorguHatasi`] döner.
pub fn tara(metin: &str) -> Result<Tarama, SahneHata> {
    let baytlar: Vec<char> = metin.chars().collect();
    let mut jetonlar: Vec<Jeton> = Vec::new();
    let mut i = 0usize;
    while i < baytlar.len() {
        let c = baytlar[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '-' && baytlar.get(i + 1) == Some(&'-') {
            while i < baytlar.len() && baytlar[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && baytlar.get(i + 1) == Some(&'*') {
            let bas = i;
            i += 2;
            while i + 1 < baytlar.len() && !(baytlar[i] == '*' && baytlar[i + 1] == '/') {
                i += 1;
            }
            if i + 1 >= baytlar.len() {
                return Err(SahneHata::SorguHatasi {
                    mesaj: "yorum bloğu kapatılmamış (/* ... */)".to_string(),
                    konum: bas,
                });
            }
            i += 2;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let bas = i;
            while i < baytlar.len() && (baytlar[i].is_alphanumeric() || baytlar[i] == '_') {
                i += 1;
            }
            let ad: String = baytlar[bas..i].iter().collect();
            jetonlar.push(Jeton::Kimlik(ad));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && baytlar.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let (jeton, sonraki) = sayi_tara(&baytlar, i)?;
            jetonlar.push(jeton);
            i = sonraki;
            continue;
        }
        if c == '\'' {
            let (deger, sonraki) = metin_tara(&baytlar, i)?;
            jetonlar.push(Jeton::Metin(deger));
            i = sonraki;
            continue;
        }
        if c == '"' || c == '`' || c == '[' {
            let kapatan = if c == '[' { ']' } else { c };
            let bas = i;
            i += 1;
            let mut ad = String::new();
            while i < baytlar.len() {
                if baytlar[i] == kapatan {
                    break;
                }
                ad.push(baytlar[i]);
                i += 1;
            }
            if i >= baytlar.len() {
                return Err(SahneHata::SorguHatasi {
                    mesaj: "tırnak içinde tanımlanan kimlik kapatılmamış".to_string(),
                    konum: bas,
                });
            }
            i += 1;
            jetonlar.push(Jeton::Kimlik(ad));
            continue;
        }
        if let Some(islec) = islec_tara(&baytlar, i) {
            jetonlar.push(Jeton::Islec(islec.0));
            i = islec.1;
            continue;
        }
        return Err(SahneHata::SorguHatasi {
            mesaj: format!("tanınmayan karakter: {c:?}"),
            konum: i,
        });
    }
    Ok(Tarama {
        jetonlar,
        son_konum: baytlar.len(),
    })
}

/// Sayı sabitini tarar.
fn sayi_tara(baytlar: &[char], bas: usize) -> Result<(Jeton, usize), SahneHata> {
    let mut i = bas;
    let mut metin = String::new();
    let mut kayan_mi = false;
    while i < baytlar.len() && baytlar[i].is_ascii_digit() {
        metin.push(baytlar[i]);
        i += 1;
    }
    if baytlar.get(i) == Some(&'.') && baytlar.get(i + 1).is_some_and(char::is_ascii_digit) {
        kayan_mi = true;
        metin.push('.');
        i += 1;
        while i < baytlar.len() && baytlar[i].is_ascii_digit() {
            metin.push(baytlar[i]);
            i += 1;
        }
    }
    if matches!(baytlar.get(i), Some('e') | Some('E')) {
        let mut j = i + 1;
        if matches!(baytlar.get(j), Some('+') | Some('-')) {
            j += 1;
        }
        if baytlar.get(j).is_some_and(char::is_ascii_digit) {
            kayan_mi = true;
            metin.push('e');
            i += 1;
            if matches!(baytlar.get(i), Some('+') | Some('-')) {
                metin.push(baytlar[i]);
                i += 1;
            }
            while i < baytlar.len() && baytlar[i].is_ascii_digit() {
                metin.push(baytlar[i]);
                i += 1;
            }
        }
    }
    if kayan_mi {
        let deger = metin.parse::<f64>().map_err(|_| SahneHata::SorguHatasi {
            mesaj: format!("geçersiz sayı sabiti: {metin}"),
            konum: bas,
        })?;
        return Ok((Jeton::KayanSayi(deger), i));
    }
    // SQLite tam sayı literalini i64'e sığmıyorsa gerçek sayıya dönüştürür.
    match metin.parse::<i64>() {
        Ok(deger) => Ok((Jeton::TamSayi(deger), i)),
        Err(_) => {
            let deger = metin.parse::<f64>().map_err(|_| SahneHata::SorguHatasi {
                mesaj: format!("geçersiz sayı sabiti: {metin}"),
                konum: bas,
            })?;
            Ok((Jeton::KayanSayi(deger), i))
        }
    }
}

/// Tek tırnaklı metin sabitini tarar (`''` kaçışı desteklenir).
fn metin_tara(baytlar: &[char], bas: usize) -> Result<(String, usize), SahneHata> {
    let mut i = bas + 1;
    let mut deger = String::new();
    while i < baytlar.len() {
        if baytlar[i] == '\'' {
            if baytlar.get(i + 1) == Some(&'\'') {
                deger.push('\'');
                i += 2;
                continue;
            }
            return Ok((deger, i + 1));
        }
        deger.push(baytlar[i]);
        i += 1;
    }
    Err(SahneHata::SorguHatasi {
        mesaj: "metin sabiti kapatılmamış (tırnak eksik)".to_string(),
        konum: bas,
    })
}

/// İşleç ve ayraçları tarar.
fn islec_tara(baytlar: &[char], i: usize) -> Option<(&'static str, usize)> {
    let c = baytlar[i];
    let sonraki = baytlar.get(i + 1).copied();
    let cift = match (c, sonraki) {
        ('=', Some('=')) => Some("=="),
        ('!', Some('=')) => Some("!="),
        ('<', Some('>')) => Some("<>"),
        ('<', Some('=')) => Some("<="),
        ('>', Some('=')) => Some(">="),
        _ => None,
    };
    if let Some(islec) = cift {
        return Some((islec, i + 2));
    }
    let tek = match c {
        '(' => "(",
        ')' => ")",
        ',' => ",",
        '.' => ".",
        ';' => ";",
        '*' => "*",
        '=' => "=",
        '<' => "<",
        '>' => ">",
        '?' => "?",
        '+' => "+",
        '-' => "-",
        '/' => "/",
        '%' => "%",
        '|' => "|",
        '&' => "&",
        _ => return None,
    };
    Some((tek, i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jetonlar(metin: &str) -> Vec<Jeton> {
        match tara(metin) {
            Ok(t) => t.jetonlar,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    #[test]
    fn secim_ve_tablo_ayristirilir() {
        let j = jetonlar("SELECT a, b FROM t");
        assert_eq!(j.len(), 6);
        assert_eq!(j[0], Jeton::Kimlik("SELECT".into()));
        assert_eq!(j[2], Jeton::Islec(","));
        assert_eq!(j[5], Jeton::Kimlik("t".into()));
    }

    #[test]
    fn anahtar_kucuk_harf_buyuk_harfe_cevirilir() {
        let t = match tara("select 1") {
            Ok(t) => t,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(t.ilk_anahtar().unwrap_or_default(), "SELECT");
    }

    #[test]
    fn sayilar_ayristirilir() {
        let j = jetonlar("1 2.5 1e3 0.5");
        assert_eq!(j[0], Jeton::TamSayi(1));
        assert_eq!(j[1], Jeton::KayanSayi(2.5));
        assert_eq!(j[2], Jeton::KayanSayi(1000.0));
        assert_eq!(j[3], Jeton::KayanSayi(0.5));
    }

    #[test]
    fn devasa_tamsayi_gercek_sayiya_duser() {
        let j = jetonlar("99999999999999999999999");
        match &j[0] {
            Jeton::KayanSayi(_) => {}
            diger => panic!("beklenmeyen belirteç: {diger:?}"),
        }
    }

    #[test]
    fn metin_sabiti_ve_kacisi() {
        let j = jetonlar("'ali''nin'");
        assert_eq!(j[0], Jeton::Metin("ali'nin".into()));
    }

    #[test]
    fn kapanmamis_metin_hata_verir() {
        let sonuc = tara("SELECT 'acik");
        assert!(matches!(sonuc, Err(SahneHata::SorguHatasi { .. })));
    }

    #[test]
    fn yorumlar_atlanir() {
        let j = jetonlar("SELECT -- burasi yorum\n a /* ve bu */ FROM t");
        assert_eq!(j.len(), 4);
    }

    #[test]
    fn kapatilamis_yorum_hata_verir() {
        assert!(tara("SELECT /* acik").is_err());
    }

    #[test]
    fn tirnakli_kimlikler_okunur() {
        let j = jetonlar("\"ad\" `yol` [kat]");
        assert_eq!(j[0], Jeton::Kimlik("ad".into()));
        assert_eq!(j[1], Jeton::Kimlik("yol".into()));
        assert_eq!(j[2], Jeton::Kimlik("kat".into()));
    }

    #[test]
    fn kapatilamis_tirnak_kimligi_hata_verir() {
        assert!(tara("\"acik").is_err());
    }

    #[test]
    fn islecler_ayristirilir() {
        let j = jetonlar("= == != <> < > <= >= , . ; *");
        let beklenen = [
            "=", "==", "!=", "<>", "<", ">", "<=", ">=", ",", ".", ";", "*",
        ];
        for (i, b) in beklenen.iter().enumerate() {
            assert_eq!(j[i], Jeton::Islec(b), "belirteç {i}");
        }
    }

    #[test]
    fn taninmayan_karakter_hata_verir() {
        assert!(tara("SELECT #").is_err());
    }

    #[test]
    fn turkce_karakterli_kimlikler_okunur() {
        let j = jetonlar("SELECT şehir FROM t");
        assert_eq!(j[1], Jeton::Kimlik("şehir".into()));
    }

    #[test]
    fn bos_metin_bos_belirtec_listesi_uretir() {
        let t = match tara("   ") {
            Ok(t) => t,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(t.jetonlar.is_empty());
        assert!(t.ilk_anahtar().is_none());
    }
}
