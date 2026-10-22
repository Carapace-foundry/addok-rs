use flate2::read::GzDecoder;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::ban::BanRecord;
use crate::error::{CoreError, Result};

/// An iterator yielding parsed `BanRecord` entries from a streaming source.
pub struct BanRecordStream<R: Read> {
    reader: csv::DeserializeRecordsIntoIter<R, BanRecord>,
}

impl<R: Read> BanRecordStream<R> {
    pub fn new(reader: csv::Reader<R>) -> Self {
        Self {
            reader: reader.into_deserialize(),
        }
    }
}

impl<R: Read> Iterator for BanRecordStream<R> {
    type Item = Result<BanRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next().map(|res| res.map_err(CoreError::from))
    }
}

/// Detects the CSV delimiter (';', ',', or '\t') by sniffing the first line.
fn detect_delimiter(first_line: &str) -> u8 {
    let semicolons = first_line.matches(';').count();
    let commas = first_line.matches(',').count();
    let tabs = first_line.matches('\t').count();

    if semicolons >= commas && semicolons >= tabs && semicolons > 0 {
        b';'
    } else if tabs > commas && tabs > 0 {
        b'\t'
    } else {
        b','
    }
}

/// Open a BAN CSV or CSV.GZ file and return a streaming iterator over `BanRecord`.
pub fn open_ban_csv<P: AsRef<Path>>(path: P) -> Result<BanRecordStream<Box<dyn Read + Send>>> {
    use std::io::Seek;
    let path_ref = path.as_ref();
    let mut file = File::open(path_ref)?;

    let mut magic = [0u8; 2];
    let is_gzip = match file.read_exact(&mut magic) {
        Ok(()) => magic == [0x1f, 0x8b],
        Err(_) => false,
    };

    file.seek(std::io::SeekFrom::Start(0))?;

    let reader: Box<dyn Read + Send> = if is_gzip {
        Box::new(GzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    open_ban_csv_reader(reader)
}

/// Create a streaming reader from any `Read` stream with automatic delimiter detection.
pub fn open_ban_csv_reader(
    reader: Box<dyn Read + Send>,
) -> Result<BanRecordStream<Box<dyn Read + Send>>> {
    let mut buf_reader = BufReader::new(reader);

    // Sniff the first line to detect delimiter
    let mut first_line = String::new();
    let delimiter = match buf_reader.read_line(&mut first_line) {
        Ok(0) => b';',
        Ok(_) => detect_delimiter(&first_line),
        Err(e) => return Err(CoreError::Io(e)),
    };

    // Reconstruct reader combining the read first line and remaining buffer
    let combined_reader = std::io::Cursor::new(first_line.into_bytes()).chain(buf_reader);

    let csv_reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(Box::new(combined_reader) as Box<dyn Read + Send>);

    Ok(BanRecordStream::new(csv_reader))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_detect_delimiter() {
        assert_eq!(
            detect_delimiter("id;numero;rep;nom_voie;code_postal;nom_commune;lon;lat"),
            b';'
        );
        assert_eq!(
            detect_delimiter("id,numero,rep,nom_voie,code_postal,nom_commune,lon,lat"),
            b','
        );
    }

    #[test]
    fn test_stream_csv_semicolon() {
        let sample = "id;numero;rep;nom_voie;code_postal;code_insee;nom_commune;x;y;lon;lat;type_position;alias;nom_afnor;libelle_acheminement;nom_complementaire\n\
75105_0001_00008;8;;Boulevard Saint-Michel;75005;75105;Paris;652100;6861500;2.3431;48.8512;entrée;;BD SAINT MICHEL;PARIS;\n\
75108_0002_00010;10;bis;Avenue des Champs-Élysées;75008;75108;Paris;649800;6863200;2.3082;48.8698;entrée;;AV DES CHAMPS ELYSEES;PARIS;";

        let stream = open_ban_csv_reader(Box::new(sample.as_bytes())).unwrap();
        let records: Vec<_> = stream.collect::<Result<Vec<_>>>().unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].nom_voie, "Boulevard Saint-Michel");
        assert_eq!(records[0].numero.as_deref(), Some("8"));
        assert_eq!(records[0].lon, Some(2.3431));
        assert_eq!(records[0].lat, Some(48.8512));

        assert_eq!(records[1].nom_voie, "Avenue des Champs-Élysées");
        assert_eq!(records[1].numero.as_deref(), Some("10"));
        assert_eq!(records[1].rep.as_deref(), Some("bis"));
    }

    #[test]
    fn test_stream_csv_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let sample = "id;numero;rep;nom_voie;code_postal;code_insee;nom_commune;x;y;lon;lat;type_position;alias;nom_afnor;libelle_acheminement;nom_complementaire\n\
75105_0001_00008;8;;Boulevard Saint-Michel;75005;75105;Paris;652100;6861500;2.3431;48.8512;entrée;;BD SAINT MICHEL;PARIS;";

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(sample.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&compressed).unwrap();
        let path = temp_file.into_temp_path();

        let stream = open_ban_csv(&path).unwrap();
        let records: Vec<_> = stream.collect::<Result<Vec<_>>>().unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].nom_voie, "Boulevard Saint-Michel");
        assert_eq!(records[0].code_postal, "75005");
    }
}
