//! SQLStage komut satırı arayüzü.
//!
//! Alt komutlar: `schema`, `query`, `tables`, `sample`, `create`, `insert`, `export`.
//! Açılan her veritabanı dosyası **salt okunur** açılır; hiçbir komut okunan dosyayı
//! değiştiremez.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use sqlstage::deger::Deger;
use sqlstage::diyagram;
use sqlstage::gecmis;
use sqlstage::hata::SahneHata;
use sqlstage::izgara::IzgaraAyari;
use sqlstage::json_cikti;
use sqlstage::ornek;
use sqlstage::plan;
use sqlstage::sema::{sema_oku, KolonTuru};
use sqlstage::sql::parser::ayikla;
use sqlstage::sql::yurutucu::{calistir, SorguSonucu};
use sqlstage::veritabani::Veritabani;
use sqlstage::yazici::{self, TabloYazimi};

/// SQLStage: gömülü SQLite dosyalarını salt okunur açan şema görüntüleyici ve SQL konsolu.
#[derive(Parser, Debug)]
#[command(
    name = "sqlstage",
    version,
    about = "SQLite dosyalarını kendi okuyucusuyla açan, salt okunur şema görüntüleyici ve SQL konsolu",
    long_about = None
)]
struct Cli {
    /// Yapılacak iş.
    #[command(subcommand)]
    komut: Komut,
}

/// Alt komutlar.
#[derive(Subcommand, Debug)]
enum Komut {
    /// Tabloları, kolonları, tipleri ve birincil anahtar bilgisini gösterir.
    Schema(SchemaArgs),
    /// Tek tabloda `SELECT` sorgusu çalıştırır ve sonucu ızgara olarak basar.
    Query(QueryArgs),
    /// Tablo listesini satır sayılarıyla birlikte gösterir.
    Tables(TablesArgs),
    /// Üç tablolu örnek veri tabanı üretir.
    Sample(SampleArgs),
    /// Yeni bir tablo içeren veritabanı dosyası üretir.
    Create(CreateArgs),
    /// Var olan bir tabloya satır ekler (dosya baştan yeniden yazılır).
    Insert(InsertArgs),
    /// Sorgu sonucunu JSON dosyasına dışa aktarır.
    Export(ExportArgs),
}

/// `schema` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct SchemaArgs {
    /// Veritabanı dosyası.
    dosya: PathBuf,
    /// ASCII şema diyagramı basar.
    #[arg(long)]
    diyagram: bool,
    /// Bütünlük denetimi sonucunu gösterir.
    #[arg(long)]
    butunluk: bool,
}

/// `query` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct QueryArgs {
    /// Veritabanı dosyası.
    dosya: PathBuf,
    /// Çalıştırılacak SQL metni; `--gecmis-no` kullanılıyorsa boş bırakılabilir.
    sorgu: Option<String>,
    /// Sorgu planı açıklamasını basar (sorguyu çalıştırmadan).
    #[arg(long)]
    plan: bool,
    /// Sonucu JSON olarak basar (ızgara yerine).
    #[arg(long)]
    json: bool,
    /// Kolon genişliği sınırı (varsayılan 28).
    #[arg(long, default_value_t = 28)]
    genislik: usize,
    /// Sorgu geçmişine yazılacak dosya (varsayılan: SQLSTAGE_HOME/sorgular.jsonl).
    #[arg(long)]
    gecmis_dosya: Option<PathBuf>,
    /// Sorgu geçmişindeki 1 tabanlı numarayla kayıtlı sorguyu yeniden çalıştırır.
    #[arg(long)]
    gecmis_no: Option<usize>,
    /// Sorgu geçmişine yazmayı kapatır.
    #[arg(long)]
    gecmis_yazma: bool,
}

/// `tables` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct TablesArgs {
    /// Veritabanı dosyası.
    dosya: PathBuf,
}

/// `sample` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct SampleArgs {
    /// Oluşturulacak veritabanı dosyası.
    dosya: PathBuf,
    /// Dosya varsa üzerine yazmayı kabul eder.
    #[arg(long)]
    zorla: bool,
}

/// `create` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct CreateArgs {
    /// Oluşturulacak veritabanı dosyası.
    dosya: PathBuf,
    /// Tablo adı.
    #[arg(long)]
    tablo: String,
    /// Kolon tanımı, örn. `id:INTEGER:pk`, `ad:TEXT:notnull`, `fiyat:REAL`.
    /// Birden fazla kez verilebilir.
    #[arg(long = "kolon", value_name = "AD:TIP[pk|notnull]")]
    kolonlar: Vec<String>,
    /// Dosya varsa üzerine yazmayı kabul eder.
    #[arg(long)]
    zorla: bool,
}

/// `insert` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct InsertArgs {
    /// Hedef veritabanı dosyası.
    dosya: PathBuf,
    /// Hedef tablo adı.
    #[arg(long)]
    tablo: String,
    /// Satır değerleri, örn. `ad=Ali yas=30` veya `ad=NULL`.
    /// Birden fazla kez verilebilir.
    #[arg(long = "deger", value_name = "KOLON=DEGER")]
    degerler: Vec<String>,
}

/// `export` alt komutunun seçenekleri.
#[derive(Args, Debug)]
struct ExportArgs {
    /// Veritabanı dosyası.
    dosya: PathBuf,
    /// Çalıştırılacak SQL metni.
    sorgu: String,
    /// JSON çıktısının yazılacağı dosya (yoksa standart çıktı).
    #[arg(long)]
    cikti: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match calistir_komut(cli) {
        Ok(cikti) => {
            print!("{cikti}");
            ExitCode::SUCCESS
        }
        Err(hata) => {
            eprintln!("hata: {hata}");
            ExitCode::FAILURE
        }
    }
}

/// Komutu çalıştırır ve standart çıktı metnini döndürür (test edilebilirlik için ayrıldı).
fn calistir_komut(cli: Cli) -> Result<String, SahneHata> {
    match cli.komut {
        Komut::Schema(args) => komut_schema(&args),
        Komut::Tables(args) => komut_tables(&args),
        Komut::Sample(args) => komut_sample(&args),
        Komut::Create(args) => komut_create(&args),
        Komut::Insert(args) => komut_insert(&args),
        Komut::Query(args) => komut_query(&args),
        Komut::Export(args) => komut_export(&args),
    }
}

fn komut_schema(args: &SchemaArgs) -> Result<String, SahneHata> {
    let db = Veritabani::ac(&args.dosya)?;
    let sema = sema_oku(&db)?;
    let mut cikti = String::new();
    cikti.push_str(&format!(
        "{} · {} sayfa · {} bayt/başlık · kodlama {:?} · WAL {}\n",
        args.dosya.display(),
        db.sayfa_sayisi(),
        db.baslik().sayfa_boyutu,
        db.baslik().kodlama,
        if db.wal_etkin_mi() {
            "etkin"
        } else {
            "kapalı"
        }
    ));

    if args.diyagram {
        cikti.push('\n');
        cikti.push_str(&diyagram::diyagram_uret(&sema));
        return Ok(cikti);
    }

    for tablo in &sema.tablolar {
        cikti.push('\n');
        cikti.push_str(&format!(
            "{}  ({} satır, kök sayfa {}, {} sayfa)\n",
            tablo.ad, tablo.satir_sayisi, tablo.kok_sayfa, tablo.taranan_sayfa
        ));
        cikti.push_str(&format!(
            "  {:<24} {:<12} {:<6} {:<8} {}\n",
            "KOLON", "TİP", "NULL", "PK", "VARSAYILAN"
        ));
        for kolon in &tablo.kolonlar {
            cikti.push_str(&format!(
                "  {:<24} {:<12} {:<6} {:<8} {}\n",
                kolon.ad,
                kolon.tur.ad(),
                if kolon.not_null { "hayır" } else { "evet" },
                if kolon.birincil_anahtar { "evet" } else { "-" },
                kolon.varsayilan.clone().unwrap_or_default()
            ));
        }
        if let Some(ilk) = tablo.birincil_anahtar().first() {
            cikti.push_str(&format!("  birincil anahtar: {ilk}\n"));
        }
    }
    if sema.tablolar.is_empty() {
        cikti.push_str("\n(şemada tablo yok)\n");
    }

    if args.butunluk {
        let rapor = db.butunluk()?;
        cikti.push_str(&format!(
            "\nBütünlük denetimi: {} sayfa, {} sorun\n",
            rapor.denetlenen_sayfa, rapor.sorun_sayisi
        ));
        for sorun in &rapor.sorunlar {
            cikti.push_str(&format!("  - {sorun}\n"));
        }
    }
    Ok(cikti)
}

fn komut_tables(args: &TablesArgs) -> Result<String, SahneHata> {
    let db = Veritabani::ac(&args.dosya)?;
    let sema = sema_oku(&db)?;
    let mut cikti = String::from("Tablo listesi:\n");
    if sema.tablolar.is_empty() {
        cikti.push_str("  (şemada tablo yok)\n");
        return Ok(cikti);
    }
    for satir in diyagram::ozet_satirlari(&sema) {
        cikti.push_str(&format!("  {satir}\n"));
    }
    for indeks in &sema.indeksler {
        cikti.push_str(&format!(
            "  {:<24} indeks ({}), kök sayfa {}\n",
            indeks.ad, indeks.tablo, indeks.kok_sayfa
        ));
    }
    Ok(cikti)
}

fn komut_sample(args: &SampleArgs) -> Result<String, SahneHata> {
    dosya_hazirligi(&args.dosya, args.zorla)?;
    let tablolar = ornek::ornek_tablolar();
    yazici::veritabani_yaz(&args.dosya, &tablolar)?;
    Ok(format!(
        "{} yazıldı: {}\n",
        args.dosya.display(),
        ornek::aciklama()
    ))
}

fn komut_create(args: &CreateArgs) -> Result<String, SahneHata> {
    dosya_hazirligi(&args.dosya, args.zorla)?;
    if args.kolonlar.is_empty() {
        return Err(SahneHata::SemaHatasi {
            mesaj: "en az bir --kolon AD:TIP vermek gerekir".to_string(),
        });
    }
    let (kolon_tanimlari, govde) = kolonlari_ayikla(&args.kolonlar)?;
    let sql = format!("CREATE TABLE {}({})", args.tablo, govde);
    let tablolar = vec![TabloYazimi {
        ad: args.tablo.clone(),
        sql,
        satirlar: Vec::new(),
    }];
    yazici::veritabani_yaz(&args.dosya, &tablolar)?;
    Ok(format!(
        "{} yazıldı: tablo `{}` ({} kolon)\n",
        args.dosya.display(),
        args.tablo,
        kolon_tanimlari.len()
    ))
}

fn komut_insert(args: &InsertArgs) -> Result<String, SahneHata> {
    if args.degerler.is_empty() {
        return Err(SahneHata::SemaHatasi {
            mesaj: "en az bir --deger KOLON=DEGER vermek gerekir".to_string(),
        });
    }
    let db = Veritabani::ac(&args.dosya)?;
    let sema = sema_oku(&db)?;
    let tablo = sema.tablo(&args.tablo).ok_or_else(|| SahneHata::TabloYok {
        ad: args.tablo.clone(),
        mevcut: sema.tablolar.iter().map(|t| t.ad.clone()).collect(),
    })?;
    let degerler = degerleri_coz(&tablo.kolonlar, tablo.rowid_alan, &args.degerler)?;

    // Dosyanın tamamı okunmuş modele çevrilir, ardından baştan yazılır. Desteklenmeyen
    // bir yapı varsa (indeks, WITHOUT ROWID) yazma başlamadan reddedilir.
    let mut tablolar = model_uret(&db, &sema)?;
    let hedef = tablolar
        .iter_mut()
        .find(|t| t.ad.eq_ignore_ascii_case(&args.tablo))
        .ok_or_else(|| SahneHata::TabloYok {
            ad: args.tablo.clone(),
            mevcut: Vec::new(),
        })?;
    let sonraki_kimlik = hedef.satirlar.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1;
    hedef.satirlar.push((sonraki_kimlik, degerler));
    let toplam = hedef.satirlar.len();
    yazici::dosya_guncelle(&args.dosya, &yazici::veritabani_goruntusu(&tablolar)?)?;
    Ok(format!(
        "`{}` tablosuna 1 satır eklendi (satır kimliği {}), toplam {toplam}\n",
        tablo.ad, sonraki_kimlik
    ))
}

fn komut_query(args: &QueryArgs) -> Result<String, SahneHata> {
    let gecmis_yolu = gecmis::gecis_dosyasi(args.gecmis_dosya.as_deref());
    let db = Veritabani::ac(&args.dosya)?;
    let sorgu_metni = match (args.gecmis_no, &args.sorgu) {
        (Some(numara), _) => gecmis::kayit_getir(&gecmis_yolu, numara)?.sorgu,
        (None, Some(metin)) if !metin.trim().is_empty() => metin.clone(),
        (None, _) => {
            return Err(SahneHata::SemaHatasi {
                mesaj: "ya sorgu metni verilmeli ya da --gecmis-no ile kayıtlı sorgu \
                        numarası belirtilmelidir"
                    .to_string(),
            })
        }
    };
    let sorgu = ayikla(&sorgu_metni)?;

    let mut cikti = String::new();
    if args.plan {
        cikti.push_str(&plan::plan_metni(&db, &sorgu)?);
        cikti.push('\n');
    }

    let sonuc = calistir(&db, &sorgu);
    match sonuc {
        Ok(sonuc) => {
            cikti.push_str(&sonucu_metin(
                &sonuc,
                &sorgu_metni,
                args.json,
                args.genislik,
            ));
            cikti.push_str(&istatistik_metin(&sonuc));
            if !args.gecmis_yazma {
                let kayit = gecmis::GecmisKaydi::basarili(
                    &args.dosya.display().to_string(),
                    &sorgu_metni,
                    sonuc.sure_ns,
                    sonuc.satir_sayisi(),
                );
                if let Err(hata) = gecmis::kaydet(&gecmis_yolu, &kayit) {
                    eprintln!("uyarı: sorgu geçmişine yazılamadı: {hata}");
                }
            }
            Ok(cikti)
        }
        Err(hata) => {
            if !args.gecmis_yazma {
                let kayit = gecmis::GecmisKaydi::basarisiz(
                    &args.dosya.display().to_string(),
                    &sorgu_metni,
                    &hata.to_string(),
                );
                if let Err(gecmis_hata) = gecmis::kaydet(&gecmis_yolu, &kayit) {
                    eprintln!("uyarı: sorgu geçmişine yazılamadı: {gecmis_hata}");
                }
            }
            Err(hata)
        }
    }
}

fn komut_export(args: &ExportArgs) -> Result<String, SahneHata> {
    let db = Veritabani::ac(&args.dosya)?;
    let sorgu = ayikla(&args.sorgu)?;
    let sonuc = calistir(&db, &sorgu)?;
    let metin = json_cikti::dogrudan(&sonuc, &args.sorgu)?;
    match &args.cikti {
        Some(yol) => {
            std::fs::write(yol, format!("{metin}\n")).map_err(|kaynak| SahneHata::Io {
                yol: yol.display().to_string(),
                kaynak,
            })?;
            Ok(format!(
                "{} yazıldı ({} satır)\n",
                yol.display(),
                sonuc.satir_sayisi()
            ))
        }
        None => Ok(format!("{metin}\n")),
    }
}

/// Sonucu istenen biçimde metne çevirir.
fn sonucu_metin(sonuc: &SorguSonucu, sorgu: &str, json: bool, genislik: usize) -> String {
    if json {
        return json_cikti::dogrudan(sonuc, sorgu)
            .unwrap_or_else(|hata| format!("JSON hatası: {hata}\n"));
    }
    sqlstage::izgara::ciz(sonuc, &IzgaraAyari::yeni(genislik))
}

/// Çalışma istatistiklerini metne çevirir.
fn istatistik_metin(sonuc: &SorguSonucu) -> String {
    format!(
        "{} satır (taranan {}, eşleşen {}), {} ms\n",
        sonuc.satir_sayisi(),
        sonuc.taranan_satir,
        sonuc.eslesen_satir,
        sonuc.sure_ns / 1_000_000
    )
}

/// Hedef dosyanın yazılmaya uygun olduğunu doğrular.
fn dosya_hazirligi(yol: &std::path::Path, zorla: bool) -> Result<(), SahneHata> {
    if yol.exists() && !zorla {
        return Err(SahneHata::SemaHatasi {
            mesaj: format!(
                "{} zaten var; üzerine yazmak için --zorla verin",
                yol.display()
            ),
        });
    }
    if let Some(ust) = yol.parent() {
        if !ust.as_os_str().is_empty() && !ust.exists() {
            return Err(SahneHata::SemaHatasi {
                mesaj: format!("{} dizini yok", ust.display()),
            });
        }
    }
    Ok(())
}

/// `AD:TIP[pk|notnull]` biçimindeki kolon tanımlarını ayrıştırır.
fn kolonlari_ayikla(tanimlar: &[String]) -> Result<(Vec<String>, String), SahneHata> {
    let hata = |m: String| SahneHata::SemaHatasi { mesaj: m };
    let mut parcalar: Vec<String> = Vec::new();
    for tanim in tanimlar {
        let bolunmus: Vec<&str> = tanim.split(':').collect();
        if bolunmus.len() < 2 {
            return Err(hata(format!(
                "kolon tanımı AD:TIP biçiminde olmalı: {tanim}"
            )));
        }
        let ad = bolunmus[0].trim();
        if ad.is_empty() {
            return Err(hata("kolon adı boş olamaz".to_string()));
        }
        let tip = bolunmus[1].trim().to_ascii_uppercase();
        if tip.is_empty() {
            return Err(hata(format!("kolon tipi boş olamaz: {tanim}")));
        }
        let mut ek = String::new();
        for bayrak in &bolunmus[2..] {
            let ust = bayrak.trim().to_ascii_uppercase();
            match ust.as_str() {
                "PK" | "PRIMARY" => ek.push_str(" PRIMARY KEY"),
                "NN" | "NOTNULL" => ek.push_str(" NOT NULL"),
                "" => {}
                diger => {
                    return Err(hata(format!(
                        "bilinmeyen kolon bayrağı: {diger} (pk, notnull)"
                    )))
                }
            }
        }
        parcalar.push(format!("{ad} {tip}{ek}"));
    }
    let govde = parcalar.join(", ");
    Ok((parcalar, govde))
}

/// `KOLON=DEGER` çiftlerini kolon sırasına göre hücre değerlerine çevirir.
///
/// `INTEGER PRIMARY KEY` (rowid alias) kolonu verilmezse `NULL` yazılır; gerçek satır
/// kimliği yazıcı tarafından atanır. Bu, SQLite'ın kendi temsilidir.
fn degerleri_coz(
    kolonlar: &[sqlstage::sema::Kolon],
    rowid_alan: Option<usize>,
    degerler: &[String],
) -> Result<Vec<Deger>, SahneHata> {
    let mut secim: Vec<(String, String)> = Vec::new();
    for deger in degerler {
        let (ad, icerik) = deger.split_once('=').ok_or_else(|| SahneHata::SemaHatasi {
            mesaj: format!("değer KOLON=DEGER biçiminde olmalı: {deger}"),
        })?;
        if icerik.contains('=') {
            return Err(SahneHata::SemaHatasi {
                mesaj: format!("değerde birden çok `=` var: {deger}"),
            });
        }
        secim.push((ad.trim().to_string(), icerik.trim().to_string()));
    }
    let mut sonuc: Vec<Deger> = Vec::with_capacity(kolonlar.len());
    for (dizin, kolon) in kolonlar.iter().enumerate() {
        match secim
            .iter()
            .find(|(ad, _)| ad.eq_ignore_ascii_case(&kolon.ad))
        {
            Some((_, icerik)) => sonuc.push(metin_coz(icerik, &kolon.tur)),
            None if rowid_alan == Some(dizin) => sonuc.push(Deger::Null),
            None => {
                return Err(SahneHata::KolonYok {
                    tablo: "<hedef tablo>".to_string(),
                    kolon: kolon.ad.clone(),
                })
            }
        }
    }
    for (ad, _) in &secim {
        if !kolonlar.iter().any(|k| k.ad.eq_ignore_ascii_case(ad)) {
            return Err(SahneHata::KolonYok {
                tablo: "<hedef tablo>".to_string(),
                kolon: ad.clone(),
            });
        }
    }
    Ok(sonuc)
}

/// Metin değerini kolon tipine göre `Deger`e çevirir.
fn metin_coz(metin: &str, tur: &KolonTuru) -> Deger {
    if metin.eq_ignore_ascii_case("NULL") {
        return Deger::Null;
    }
    match tur {
        KolonTuru::Tamsayi | KolonTuru::Sayisal => match metin.parse::<i64>() {
            Ok(deger) => Deger::Tam(deger),
            Err(_) => Deger::Metin(metin.to_string()),
        },
        KolonTuru::Gercek => match metin.parse::<f64>() {
            Ok(deger) => Deger::Gercek(deger),
            Err(_) => Deger::Metin(metin.to_string()),
        },
        _ => Deger::Metin(metin.to_string()),
    }
}

/// Okunmuş veritabanını yeniden yazılabilir tablo modeline çevirir.
///
/// # Hatalar
///
/// İndeks varsa veya tablo satır içeriği okunamıyorsa hata döner; yazma başlamaz.
fn model_uret(db: &Veritabani, sema: &sqlstage::sema::Sema) -> Result<Vec<TabloYazimi>, SahneHata> {
    if !sema.indeksler.is_empty() {
        return Err(SahneHata::Desteklenmiyor {
            ozellik: format!("{} indeksi", sema.indeksler[0].ad),
            sebep: "indeks içeren veritabanları yeniden yazılamaz; veri korunması için yazma iptal edildi",
        });
    }
    let mut tablolar = Vec::with_capacity(sema.tablolar.len());
    for tablo in &sema.tablolar {
        let satirlar = db.tablo_satirlari(tablo.kok_sayfa)?;
        let kayitlar = satirlar
            .into_iter()
            .map(|s| (s.satir_kimligi, s.degerler))
            .collect();
        tablolar.push(TabloYazimi {
            ad: tablo.ad.clone(),
            sql: tablo.sql.clone(),
            satirlar: kayitlar,
        });
    }
    Ok(tablolar)
}
