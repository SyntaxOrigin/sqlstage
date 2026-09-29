//! SQLite kayıt serileştirmesi: serial type başlığı ve hücre değerleri.
//!
//! Bir kayıt iki bölgeden oluşur: bir başlık (kayıt başlığının bayt uzunluğu + her sütun
//! için bir serial type varint'ı) ve bir gövde (değer baytları). Metin ve blob
//! değerleri sayfaya sığmayacak kadar büyükse gövdenin bir kısmı "varışma" (overflow)
//! sayfalarına taşınır ve ilk dört baytta sonraki varışma sayfasının numarası bulunur.
//!
//! Kaynak: <https://www.sqlite.org/fileformat2.html#record_format>

use std::collections::HashSet;

use crate::baslik::MetinKodlamasi;
use crate::deger::Deger;
use crate::hata::SahneHata;
use crate::sayfa::Hucre;

/// Bir hücrenin tam yükünü (yerel parça + varışma zinciri) bir araya getirir.
///
/// `sayfa_oku`, verilen sayfa numarasının kullanılabilir kısmını döndürür. Varışma
/// sayfasının ilk dört baytı bir sonraki sayfanın numarasıdır; kalan `U - 4` bayt veri
/// taşır.
///
/// # Hatalar
///
/// Zincir kopuk (0 numarasına geçilmiş), dosya dışına çıkan bir sayfaya atlamış, döngüye
/// girmiş veya verinin gerektiğinden uzun olduğu durumlarda hata döner.
pub fn yuk_birlestir<F>(
    ilk_sayfa: u32,
    hucre: &Hucre,
    mut sayfa_oku: F,
) -> Result<Vec<u8>, SahneHata>
where
    F: FnMut(u32) -> Result<Vec<u8>, SahneHata>,
{
    // `bildirilen_yuk` güvenilmeyen dosyadan gelen ham bir varint'tir ve üst sınırı
    // yoktur; bu yüzden bellek ön-tahsisi için kullanılamaz. Ön-tahmin yapılsa
    // `i64::MAX` boyutlu tek bir hücre `handle_alloc_error` ile süreci düşürürdü.
    // `toplam` yalnızca bir denetim değeridir: veri, her varışma sayfası geldiğinde
    // o bloğu kapsayacak kadar büyütülür (blok blok büyüme).
    let toplam = usize::try_from(hucre.bildirilen_yuk.max(0)).unwrap_or(0);
    let mut veri = Vec::new();
    veri.extend_from_slice(&hucre.yerel);

    let mut sonraki = hucre.varisma_sayfasi;
    let mut ziyaret = HashSet::from([ilk_sayfa]);
    let mut adim = 0usize;
    while veri.len() < toplam {
        let sayfa_no = sonraki.ok_or_else(|| SahneHata::BozukVarisma {
            ayrinti: format!(
                "yük {toplam} bayt bildirilmiş ama {}. bayttan sonra varışma sayfası yok",
                veri.len()
            ),
        })?;
        if sayfa_no == 0 {
            return Err(SahneHata::BozukVarisma {
                ayrinti: "varışma zinciri 0 (boş) sayfasına devam ediyor".to_string(),
            });
        }
        if !ziyaret.insert(sayfa_no) {
            return Err(SahneHata::BozukVarisma {
                ayrinti: format!("varışma zinciri {sayfa_no} sayfasında döngüye giriyor"),
            });
        }
        adim += 1;
        if adim > 1_000_000 {
            return Err(SahneHata::BozukVarisma {
                ayrinti: "varışma zinciri aşırı uzun".to_string(),
            });
        }
        let sayfa = sayfa_oku(sayfa_no)?;
        if sayfa.len() < 4 {
            return Err(SahneHata::BozukVarisma {
                ayrinti: format!("varışma sayfası {sayfa_no} 4 bayttan kısa"),
            });
        }
        sonraki = Some(u32::from_be_bytes([sayfa[0], sayfa[1], sayfa[2], sayfa[3]]));
        let alinacak = (toplam - veri.len()).min(sayfa.len() - 4);
        // Ön-tahmin yalnızca eldeki blokla sınırlıdır: `alinacak` gerçekten okunacak
        // bayt sayısıdır, bildirilen yük boyutu değil.
        veri.reserve(alinacak);
        veri.extend_from_slice(&sayfa[4..4 + alinacak]);
    }
    veri.truncate(toplam);
    Ok(veri)
}

/// Bir kayıt gövdesini hücre değerlerine ayrıştırır.
///
/// `kodlama`, metin alanlarının hangi kodlamayla çözüleceğini belirler.
///
/// # Hatalar
///
/// Kayıt başlığı eksik/taşmalı, serial type bilinmez (10, 11) veya gövde boyutu
/// bildirilen değerlerle tutarsızsa hata döner.
pub fn kayit_ayikla(yuk: &[u8], kodlama: MetinKodlamasi) -> Result<Vec<Deger>, SahneHata> {
    let (baslik_uzunlugu, mut konum) =
        crate::varint::varint_oku(yuk, 0).map_err(|hata| SahneHata::BozukKayit {
            ayrinti: format!("kayıt başlığı okunamadı: {hata}"),
        })?;
    if baslik_uzunlugu < 1 || baslik_uzunlugu as usize > yuk.len() {
        return Err(SahneHata::BozukKayit {
            ayrinti: format!(
                "kayıt başlığı {baslik_uzunlugu} bayt bildiriyor ama veri {} bayt",
                yuk.len()
            ),
        });
    }
    let baslik_sonu = baslik_uzunlugu as usize;

    let mut seriteller = Vec::new();
    while konum < baslik_sonu {
        let (serit, sonraki) =
            crate::varint::varint_oku(&yuk[..baslik_sonu], konum).map_err(|hata| {
                SahneHata::BozukKayit {
                    ayrinti: format!("serial type okunamadı: {hata}"),
                }
            })?;
        konum = sonraki;
        seriteller.push(serit);
    }

    let mut degerler = Vec::with_capacity(seriteller.len());
    let mut govde = baslik_sonu;
    for serit in seriteller {
        let (deger, harcanan) = tek_deger(serit, &yuk[govde..], kodlama)?;
        degerler.push(deger);
        govde += harcanan;
    }
    Ok(degerler)
}

/// Değerleri geçerli bir SQLite kayıt yüküne çevirir (yazarın kullandığı kodlayıcı).
///
/// Tamsayılar en kısa temsille, metinler UTF-8, gerçekler IEEE-754 çift duyarlılıkta
/// yazılır. `1` ve `0` değerleri ayrılmış serial type 9/8 ile kodlanır.
pub fn kayit_yaz(degerler: &[Deger]) -> Vec<u8> {
    let mut seriteller: Vec<i64> = Vec::with_capacity(degerler.len());
    let mut govde: Vec<u8> = Vec::new();
    for deger in degerler {
        match deger {
            Deger::Null => seriteller.push(0),
            Deger::Tam(0) => seriteller.push(8),
            Deger::Tam(1) => seriteller.push(9),
            Deger::Tam(i) => {
                let (serit, baytlar) = en_kisa_tamsayi(*i);
                seriteller.push(serit);
                govde.extend(baytlar);
            }
            Deger::Gercek(f) => {
                seriteller.push(7);
                govde.extend(f.to_be_bytes());
            }
            Deger::Metin(s) => {
                seriteller.push(13 + 2 * s.len() as i64);
                govde.extend(s.as_bytes());
            }
            Deger::Blob(b) => {
                seriteller.push(12 + 2 * b.len() as i64);
                govde.extend(b);
            }
        }
    }
    let mut baslik: Vec<u8> = Vec::with_capacity(seriteller.len() * 2);
    for s in &seriteller {
        baslik.extend(crate::varint::varint_yaz(*s));
    }
    // Kayıt başlığı uzunluğu, kendi varint kodlamasını da kapsar; sabit nokta iterasyonu.
    let mut baslik_uzunlugu = baslik.len() + 1;
    for _ in 0..4 {
        let aday = baslik.len() + crate::varint::varint_uzunluk(baslik_uzunlugu as i64);
        if aday == baslik_uzunlugu {
            break;
        }
        baslik_uzunlugu = aday;
    }
    let mut sonuc = crate::varint::varint_yaz(baslik_uzunlugu as i64);
    sonuc.extend(baslik);
    sonuc.extend(govde);
    sonuc
}

/// Bir `i64` değerini SQLite'in kullandığı en kısa 1/2/3/4/6/8 baytlık biçimde yazar.
///
/// Serial type 5 (6 bayt) da kullanılabilir; döngü sırası 1, 2, 3, 4, 6, 8'dir çünkü
/// serial type değerleri 5'i atlarken 6'yı kullanır.
fn en_kisa_tamsayi(deger: i64) -> (i64, Vec<u8>) {
    let tam = (deger as u64).to_be_bytes();
    // (bayt sayısı, serial type): serial type 5 altı bayt, 6 sekiz bayt anlamına gelir.
    for (bayt, serial) in [(1usize, 1i64), (2, 2), (3, 3), (4, 4), (6, 5), (8, 6)] {
        if bayt == 8 {
            return (serial, tam.to_vec());
        }
        let bit = 8 * bayt - 1;
        let en_kucuk = -(1i64 << bit);
        let en_buyuk = (1i64 << bit) - 1;
        if (en_kucuk..=en_buyuk).contains(&deger) {
            return (serial, tam[8 - bayt..].to_vec());
        }
    }
    (6, tam.to_vec())
}

/// Tek bir serial type için gövdeden değeri okur ve harcadığı bayt sayısını döndürür.
fn tek_deger(
    serit: i64,
    govde: &[u8],
    kodlama: MetinKodlamasi,
) -> Result<(Deger, usize), SahneHata> {
    let oku = |n: usize| -> Result<Vec<u8>, SahneHata> {
        if govde.len() < n {
            return Err(SahneHata::BozukKayit {
                ayrinti: format!(
                    "serial type {serit} için {n} bayt gerekiyor, {} bayt var",
                    govde.len()
                ),
            });
        }
        Ok(govde[..n].to_vec())
    };

    let deger = match serit {
        0 => Deger::Null,
        1..=6 => {
            // Serial type 5 altı bayt, 6 sekiz bayt anlamına gelir.
            let bayt_sayisi = tamsayi_bayt_sayisi(serit);
            let baytlar = oku(bayt_sayisi)?;
            let mut isaretsiz: u64 = 0;
            for b in &baytlar {
                isaretsiz = (isaretsiz << 8) | u64::from(*b);
            }
            // İşaret uzantısı yalnızca 8 bayttan kısa temsillerde gereklidir; 8 baytın
            // tamamı zaten doğrudan `i64` olarak yorumlanır.
            let deger = if bayt_sayisi == 8 {
                isaretsiz as i64
            } else {
                let bit = 8 * bayt_sayisi - 1;
                if isaretsiz & (1u64 << bit) != 0 {
                    (isaretsiz as i64) - (1i64 << (bit + 1))
                } else {
                    isaretsiz as i64
                }
            };
            Deger::Tam(deger)
        }
        7 => {
            let baytlar = oku(8)?;
            let mut tam = [0u8; 8];
            tam.copy_from_slice(&baytlar);
            Deger::Gercek(f64::from_be_bytes(tam))
        }
        8 => Deger::Tam(0),
        9 => Deger::Tam(1),
        10 | 11 => {
            return Err(SahneHata::BozukKayit {
                ayrinti: format!("ayrılmış serial type {serit} kullanıldı"),
            })
        }
        s if s >= 12 && s % 2 == 0 => Deger::Blob(oku(((s - 12) / 2) as usize)?),
        s => Deger::Metin(metin_coz(&oku(((s - 13) / 2) as usize)?, kodlama)?),
    };

    let harcanan = match serit {
        0 | 8 | 9 | 10 | 11 => 0,
        1..=6 => tamsayi_bayt_sayisi(serit),
        7 => 8,
        s => ((s - if s % 2 == 0 { 12 } else { 13 }) / 2) as usize,
    };
    Ok((deger, harcanan))
}

/// Bir tam sayı serial type'ının gövdede kapladığı bayt sayısı.
///
/// SQLite'da serial type 1–4 sırasıyla 1, 2, 3, 4 bayt; 5 altı bayt, 6 sekiz bayttır.
fn tamsayi_bayt_sayisi(serit: i64) -> usize {
    match serit {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 6,
        _ => 8,
    }
}

/// Başlıktaki kodlamaya göre metin baytlarını `String`e çevirir.
///
/// Geçersiz UTF-8 dizisi hata üretir: bozuk metni "değiştirme işareti" ile sessizce
/// yamamak, kullanıcıya yanlış veri göstermekten daha tehlikelidir.
pub fn metin_coz(baytlar: &[u8], kodlama: MetinKodlamasi) -> Result<String, SahneHata> {
    match kodlama {
        MetinKodlamasi::Utf8 => {
            String::from_utf8(baytlar.to_vec()).map_err(|hata| SahneHata::BozukKayit {
                ayrinti: format!("geçersiz UTF-8 metni: {hata}"),
            })
        }
        MetinKodlamasi::Utf16Le => utf16_coz(baytlar, false),
        MetinKodlamasi::Utf16Be => utf16_coz(baytlar, true),
    }
}

/// UTF-16 bayt dizisini `String`e çevirir (`buyuk_endian` bayt sırasını belirler).
fn utf16_coz(baytlar: &[u8], buyuk_endian: bool) -> Result<String, SahneHata> {
    if baytlar.len() % 2 != 0 {
        return Err(SahneHata::BozukKayit {
            ayrinti: format!(
                "UTF-16 metin çift sayıda bayt içermeli, {} bayt",
                baytlar.len()
            ),
        });
    }
    let birimler: Vec<u16> = baytlar
        .chunks_exact(2)
        .map(|c| {
            if buyuk_endian {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16(&birimler).map_err(|hata| SahneHata::BozukKayit {
        ayrinti: format!("geçersiz UTF-16 metni: {hata}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    /// Varışma zinciri test verisinin `i` indisindeki baytı (üretilebilir bir desen).
    fn desen_bayti(i: usize) -> u8 {
        (i % 251) as u8
    }

    /// `ilk` sayfa numarasından başlayarak `adet` adet varışma sayfası üretir.
    ///
    /// Her sayfa `blok` bayt veri taşır; ilk dört baytında sonraki sayfanın numarası
    /// (büyük uçlu) bulunur, son sayfada bu alan `0`'dır. Veri, `bas` indisinden
    /// başlayarak `desen_bayti` deseniyle doldurulur.
    fn varisma_sayfalari(ilk: u32, blok: usize, adet: usize, bas: usize) -> HashMap<u32, Vec<u8>> {
        let mut sayfalar = HashMap::new();
        for i in 0..adet {
            let no = ilk + i as u32;
            let sonraki = if i + 1 < adet { ilk + i as u32 + 1 } else { 0 };
            let mut sayfa = Vec::with_capacity(blok + 4);
            sayfa.extend_from_slice(&sonraki.to_be_bytes());
            for j in 0..blok {
                sayfa.push(desen_bayti(bas + i * blok + j));
            }
            sayfalar.insert(no, sayfa);
        }
        sayfalar
    }

    /// Zinciri üreten hücreyi kurar: `yerel` bayt yerel yük + `varisma` sayfa numarası.
    fn zincir_hucresi(bildirilen_yuk: i64, yerel: Vec<u8>, varisma: Option<u32>) -> Hucre {
        Hucre {
            ofset: 0,
            sol_cocuk: None,
            anahtar: 1,
            bildirilen_yuk,
            yerel,
            varisma_sayfasi: varisma,
        }
    }

    #[test]
    fn tum_tipler_gidis_gelir() {
        let girdi = vec![
            Deger::Null,
            Deger::Tam(0),
            Deger::Tam(1),
            Deger::Tam(42),
            Deger::Tam(-42),
            Deger::Tam(1_000_000_000),
            Deger::Tam(i64::MIN),
            Deger::Tam(i64::MAX),
            Deger::Gercek(3.5),
            Deger::Gercek(-0.125),
            Deger::Metin("merhaba dünya".into()),
            Deger::Metin(String::new()),
            Deger::Blob(vec![0, 1, 2, 255]),
            Deger::Blob(Vec::new()),
        ];
        let cikan = match kayit_ayikla(&kayit_yaz(&girdi), MetinKodlamasi::Utf8) {
            Ok(d) => d,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cikan, girdi);
    }

    #[test]
    fn en_kisa_tamsayi_temsilleri() {
        for deger in [
            0i64,
            1,
            127,
            128,
            -1,
            -128,
            -129,
            32_767,
            32_768,
            8_388_607,
            i64::MIN,
        ] {
            let (serit, baytlar) = en_kisa_tamsayi(deger);
            assert!(matches!(serit, 1..=6), "deger {deger} için serial {serit}");
            assert_eq!(baytlar.len(), tamsayi_bayt_sayisi(serit), "deger {deger}");
        }
        assert_eq!(en_kisa_tamsayi(0).0, 1);
        assert_eq!(en_kisa_tamsayi(127).0, 1);
        assert_eq!(en_kisa_tamsayi(128).0, 2);
        assert_eq!(en_kisa_tamsayi(-1).0, 1);
        assert_eq!(en_kisa_tamsayi(-128).0, 1);
        assert_eq!(en_kisa_tamsayi(-129).0, 2);
        assert_eq!(en_kisa_tamsayi(32_767).0, 2);
        assert_eq!(en_kisa_tamsayi(32_768).0, 3);
        assert_eq!(en_kisa_tamsayi(i64::MIN).0, 6);
        assert_eq!(en_kisa_tamsayi(2_147_483_648).0, 5);
        assert_eq!(en_kisa_tamsayi(32_768).1.len(), 3);
    }

    #[test]
    fn bos_kayit_sifir_deger_uretir() {
        let yuk = kayit_yaz(&[]);
        let cikan = match kayit_ayikla(&yuk, MetinKodlamasi::Utf8) {
            Ok(d) => d,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert!(cikan.is_empty());
    }

    #[test]
    fn ayrilmis_serial_type_reddedilir() {
        let mut yuk = crate::varint::varint_yaz(2);
        yuk.push(10);
        assert!(matches!(
            kayit_ayikla(&yuk, MetinKodlamasi::Utf8),
            Err(SahneHata::BozukKayit { .. })
        ));
    }

    #[test]
    fn baslik_uzunlugu_siniri_tasan_kayit_reddedilir() {
        let mut yuk = crate::varint::varint_yaz(50);
        yuk.push(0);
        assert!(kayit_ayikla(&yuk, MetinKodlamasi::Utf8).is_err());
    }

    #[test]
    fn govde_kisaliyken_hata_verir() {
        // TEXT serial type 13 + 2*10 = 33, ama gövdede yalnızca 3 bayt var.
        let mut baslik = crate::varint::varint_yaz(2);
        baslik.push(33);
        baslik.extend_from_slice(b"abc");
        assert!(kayit_ayikla(&baslik, MetinKodlamasi::Utf8).is_err());
    }

    #[test]
    fn gecersiz_utf8_reddedilir() {
        let mut baslik = crate::varint::varint_yaz(2);
        baslik.push(13 + 2); // 2 bayt TEXT
        baslik.extend_from_slice(&[0xFF, 0xFE]);
        assert!(matches!(
            kayit_ayikla(&baslik, MetinKodlamasi::Utf8),
            Err(SahneHata::BozukKayit { .. })
        ));
    }

    #[test]
    fn utf16_kodlamalari_cozulur() {
        let le: Vec<u8> = "AB".encode_utf16().flat_map(|b| b.to_le_bytes()).collect();
        assert_eq!(
            metin_coz(&le, MetinKodlamasi::Utf16Le).unwrap_or_default(),
            "AB"
        );
        let be: Vec<u8> = "AB".encode_utf16().flat_map(|b| b.to_be_bytes()).collect();
        assert_eq!(
            metin_coz(&be, MetinKodlamasi::Utf16Be).unwrap_or_default(),
            "AB"
        );
    }

    #[test]
    fn tek_bayt_artan_utf16_reddedilir() {
        assert!(metin_coz(&[0x41], MetinKodlamasi::Utf16Le).is_err());
    }

    #[test]
    fn varisma_birlestir_zinciri_okur() {
        let hucre = Hucre {
            ofset: 0,
            sol_cocuk: None,
            anahtar: 1,
            bildirilen_yuk: 9,
            yerel: vec![1, 2, 3],
            varisma_sayfasi: Some(5),
        };
        let mut sayfalar = std::collections::HashMap::new();
        sayfalar.insert(5u32, vec![0, 0, 0, 6, 4, 5]);
        sayfalar.insert(6u32, vec![0, 0, 0, 0, 7, 8, 9, 10]);
        let sonuc = match yuk_birlestir(2, &hucre, |no| {
            sayfalar.get(&no).cloned().ok_or(SahneHata::BozukVarisma {
                ayrinti: format!("sayfa {no} yok"),
            })
        }) {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(sonuc, vec![1, 2, 3, 4, 5, 7, 8, 9, 10]);
    }

    #[test]
    fn varisma_kopuk_zincir_hata_verir() {
        let hucre = Hucre {
            ofset: 0,
            sol_cocuk: None,
            anahtar: 1,
            bildirilen_yuk: 10,
            yerel: vec![1],
            varisma_sayfasi: Some(5),
        };
        let sonuc = yuk_birlestir(2, &hucre, |_no| Ok(vec![0, 0, 0, 0, 2, 3, 4, 5]));
        assert!(matches!(sonuc, Err(SahneHata::BozukVarisma { .. })));
    }

    #[test]
    fn varisma_dongusu_yakalanir() {
        let hucre = Hucre {
            ofset: 0,
            sol_cocuk: None,
            anahtar: 1,
            bildirilen_yuk: 40,
            yerel: vec![0; 4],
            varisma_sayfasi: Some(5),
        };
        let sonuc = yuk_birlestir(2, &hucre, |no| {
            // 5 -> 6 -> 5 döngüsü (sonraki sayfa numarası büyük uçlu yazılır).
            if no == 5 {
                Ok(vec![0, 0, 0, 6, 1])
            } else {
                Ok(vec![0, 0, 0, 5, 1])
            }
        });
        assert!(matches!(sonuc, Err(SahneHata::BozukVarisma { .. })));
    }

    #[test]
    fn varisma_sifir_sayfasina_devam_eder() {
        let hucre = Hucre {
            ofset: 0,
            sol_cocuk: None,
            anahtar: 1,
            bildirilen_yuk: 10,
            yerel: vec![0; 2],
            varisma_sayfasi: Some(5),
        };
        let sonuc = yuk_birlestir(2, &hucre, |_no| Ok(vec![0, 0, 0, 0, 9]));
        assert!(matches!(sonuc, Err(SahneHata::BozukVarisma { .. })));
    }

    #[test]
    fn varisma_sayfasi_cok_kisa_hata_verir() {
        let hucre = Hucre {
            ofset: 0,
            sol_cocuk: None,
            anahtar: 1,
            bildirilen_yuk: 10,
            yerel: vec![0; 2],
            varisma_sayfasi: Some(5),
        };
        let sonuc = yuk_birlestir(2, &hucre, |_no| Ok(vec![0, 0]));
        assert!(matches!(sonuc, Err(SahneHata::BozukVarisma { .. })));
    }

    #[test]
    fn buyuk_varisma_kaydi_blok_blok_okunur() {
        // Çıktı tamponu tek seferde değil, her varışma sayfası geldiğinde o blok kadar
        // büyür (zlib'in çıktı tamponu deseni). 120 000 baytlık yük 30 sayfalık bir
        // zincirden geçerek bayt bayt doğru okunmalıdır.
        const YEREL: usize = 100;
        const BLOK: usize = 4092;
        const ADET: usize = 30;
        const TOPLAM: usize = 120_000;

        let yerel: Vec<u8> = (0..YEREL).map(desen_bayti).collect();
        let sayfalar = varisma_sayfalari(5, BLOK, ADET, YEREL);
        let hucre = zincir_hucresi(TOPLAM as i64, yerel, Some(5));

        let sonuc = yuk_birlestir(2, &hucre, |no| {
            sayfalar.get(&no).cloned().ok_or(SahneHata::BozukVarisma {
                ayrinti: format!("sayfa {no} yok"),
            })
        });
        let veri = match sonuc {
            Ok(v) => v,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(veri.len(), TOPLAM);
        let beklenen: Vec<u8> = (0..TOPLAM).map(desen_bayti).collect();
        assert_eq!(veri, beklenen);
    }

    #[test]
    fn asiri_buyuk_bildirilen_yuk_araci_dusurmez() {
        // Regresyon: `bildirilen_yuk` doğrudan `Vec::with_capacity`ye veriliyordu.
        // 64-bit'te `usize::try_from(i64::MAX)` başarılı olduğu için tek bir bozuk
        // dosya `handle_alloc_error` ile süreci düşürüyordu. Artık değer yalnızca
        // denetim sayısıdır: zincir tükenince kontrollü hata döner.
        for bildirilen in [i64::MAX, 1i64 << 62, 1i64 << 40, 1i64 << 31] {
            let hucre = zincir_hucresi(bildirilen, vec![1, 2, 3], Some(5));
            let sonuc = yuk_birlestir(2, &hucre, |no| {
                if no == 5 {
                    Ok(vec![0, 0, 0, 6, 4, 5])
                } else {
                    Ok(vec![0, 0, 0, 0, 7])
                }
            });
            let hata = match sonuc {
                Ok(veri) => panic!(
                    "bildirilen yük {bildirilen} kabul edildi: {} bayt üretildi",
                    veri.len()
                ),
                Err(hata) => hata,
            };
            assert!(
                matches!(hata, SahneHata::BozukVarisma { .. }),
                "bildirilen yük {bildirilen} için beklenmeyen hata: {hata:?}"
            );
        }
    }

    #[test]
    fn negatif_bildirilen_yuk_bos_yuk_uretir() {
        // Negatif (ve sıfır) boyut alanları sınır dışıdır: okuma sınırı olarak
        // kullanılmaz, toplam sıfır kabul edilir ve tahsis yapılmaz.
        for bildirilen in [-1i64, i64::MIN, 0] {
            let hucre = zincir_hucresi(bildirilen, vec![9; 4], Some(5));
            let sonuc = match yuk_birlestir(2, &hucre, |_no| Ok(vec![0; 16])) {
                Ok(v) => v,
                Err(hata) => panic!("beklenmeyen hata: {hata}"),
            };
            assert!(
                sonuc.is_empty(),
                "bildirilen yük {bildirilen} için {} bayt döndü",
                sonuc.len()
            );
        }
    }

    #[test]
    fn varisma_dongusu_kisa_yuklu_hucrede_de_yakalanir() {
        // Döngü koruması `toplam` boyutundan bağımsız çalışır: bildirilen yük
        // zincirin sunabileceğinden büyük olsa bile zincir yeniden ziyaret edilirse
        // hata döner (aksi hâlde sonsuz döngü + bellek taşması olurdu).
        let hucre = zincir_hucresi(1_000_000, vec![0; 4], Some(5));
        let sonuc = yuk_birlestir(2, &hucre, |no| {
            // 5 -> 6 -> 5
            if no == 5 {
                Ok(vec![0, 0, 0, 6, 1])
            } else {
                Ok(vec![0, 0, 0, 5, 1])
            }
        });
        let hata = match sonuc {
            Ok(v) => panic!("döngü kabul edildi: {} bayt", v.len()),
            Err(hata) => hata,
        };
        assert!(
            matches!(hata, SahneHata::BozukVarisma { .. }),
            "beklenmeyen hata: {hata:?}"
        );
    }
}
