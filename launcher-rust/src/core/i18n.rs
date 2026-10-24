//! The 14 launcher languages: text lookup, language detection and small helpers.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::platform;

/// Launcher languages, in the order the interface lists them.
pub const LANGS: [&str; 14] = [
    "en", "ru", "ar", "es", "pt", "fr", "de", "it", "pl", "tr", "zh", "ja", "ko", "id",
];

/// Suffix marking a warning line; it is not translated.
const WARN: &str = "  ⚠";

/// A language code from [`LANGS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lang(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Americas,
    Europe,
    Asia,
}

#[derive(Deserialize, Default)]
struct Trophies {
    #[serde(default)]
    texts: HashMap<String, Vec<(String, String)>>,
    #[serde(default)]
    ui: HashMap<String, HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct Table {
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    translations: HashMap<String, Vec<String>>,
    #[serde(default)]
    extra: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    ru: HashMap<String, String>,
    #[serde(default)]
    windows_languages: HashMap<String, String>,
    #[serde(default)]
    trophies: Trophies,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table: Table =
            serde_json::from_str(include_str!("../../assets/i18n.json")).unwrap_or_default();
        table.index = table
            .keys
            .iter()
            .enumerate()
            .map(|(i, key)| (key.clone(), i))
            .collect();
        table
    })
}

/// Same order as the Python table: the extra texts win over the positional ones.
fn lookup(lang: &str, en: &str) -> Option<String> {
    let table = table();
    if lang == "ru" {
        return table.ru.get(en).cloned();
    }
    let extra = table.extra.get(lang).and_then(|extra| extra.get(en));
    let positional = || {
        let position = *table.index.get(en)?;
        table.translations.get(lang)?.get(position)
    };
    extra
        .filter(|text| !text.is_empty())
        .or_else(|| positional().filter(|text| !text.is_empty()))
        .cloned()
}

/// Rust-only UI strings absent from the Python tables, as (English, [(language, text)]). Other
/// languages fall back to English: a translator gap to fill.
static UI_EXTRA: &[(&str, &[(&str, &str)])] = &[
    (
        "Graphics card not checked: the game picks one.",
        &[("pt", "Placa de vídeo não verificada: o jogo escolhe uma.")],
    ),
    (
        "The port folder is incomplete. Missing from {}:\n\n{}\n\nDownload the package again from the releases page and extract all of it; the launcher must stay inside that folder.",
        &[("pt", "A pasta do port está incompleta. Faltando em {}:\n\n{}\n\nBaixe o pacote de novo na página de releases e extraia tudo; o launcher precisa ficar dentro dessa pasta.")],
    ),
    (
        "From a source checkout, build.sh builds them.",
        &[("pt", "Num checkout do código-fonte, o build.sh gera esses arquivos.")],
    ),
    (
        "Missing or empty files:",
        &[("pt", "Arquivos ausentes ou vazios:")],
    ),
    (
        "Initial setup",
        &[("pt", "Configuração inicial")],
    ),
    (
        "Step {} of {}",
        &[("pt", "Passo {} de {}")],
    ),
    (
        "Video",
        &[("pt", "Vídeo")],
    ),
    (
        "Skip",
        &[("pt", "Pular")],
    ),
    (
        "Back",
        &[("pt", "Voltar")],
    ),
    (
        "Next",
        &[("pt", "Avançar")],
    ),
    (
        "Finish",
        &[("pt", "Concluir")],
    ),
    (
        "Choose the folder of your game dump. The update folder is optional.",
        &[("pt", "Escolha a pasta do dump do jogo. A pasta da atualização é opcional.")],
    ),
    (
        "The game runs on this graphics card.",
        &[("pt", "O jogo roda nesta placa de vídeo.")],
    ),
    (
        "Resolution and frame rate. The in-game menu (Insert or R3+L2) changes the same values.",
        &[("pt", "Resolução e taxa de quadros. O menu do jogo (Insert ou R3+L2) muda os mesmos valores.")],
    ),
    (
        "You can change all of this later in the Graphics, Performance and Game tabs.",
        &[("pt", "Tudo isso pode ser mudado depois nas abas Graphics, Performance e Game.")],
    ),
    (
        "{} lines kept",
        &[
            ("pt", "{} linhas mantidas"),
            ("es", "{} líneas guardadas"),
            ("ru", "{} строк хранится"),
        ],
    ),
    (
        "Open last_run.log",
        &[
            ("pt", "Abrir last_run.log"),
            ("es", "Abrir last_run.log"),
            ("ru", "Открыть last_run.log"),
        ],
    ),
    (
        "Americas",
        &[("pt", "Américas"), ("es", "América"), ("ru", "Америка")],
    ),
    (
        "Europe",
        &[("pt", "Europa"), ("es", "Europa"), ("ru", "Европа")],
    ),
    (
        "Asia & Middle East",
        &[
            ("pt", "Ásia e Oriente Médio"),
            ("es", "Asia y Oriente Medio"),
            ("ru", "Азия и Ближний Восток"),
        ],
    ),
    (
        "These files could not be read:\n\n{}\n\nThe launcher shows default values for what it could not read. The files stay as they are until you save (Save or PLAY), which writes what it shows.",
        &[(
            "pt",
            "Não foi possível ler estes arquivos:\n\n{}\n\nO launcher mostra valores padrão no lugar do que não conseguiu ler. Os arquivos ficam como estão até você salvar (Salvar ou JOGAR), o que grava o que ele mostra.",
        )],
    ),
    (
        "mods_presets.json could not be read, so no preset is listed and the file is left as it is. Fix or delete it to save presets again.",
        &[(
            "pt",
            "Não foi possível ler mods_presets.json; nenhum preset é listado e o arquivo fica como está. Corrija ou apague o arquivo para voltar a salvar presets.",
        )],
    ),
    // The drawer subtitles (ruling R56: the mockup's English and Portuguese). The Graphics and
    // Game ones are keys of the JSON table, and the Trophies and Mods ones are among its extra
    // texts in Portuguese, which `tr` reads first (the Trophies one in the 2.6.0 wording).
    (
        "Frame rate, shaders and GPU data paths. Applied when the game starts.",
        &[(
            "pt",
            "Taxa de quadros, shaders e caminhos de dados da GPU. Aplicado ao iniciar o jogo.",
        )],
    ),
    (
        "Keyboard, mouse and gamepad mapping.",
        &[("pt", "Mapeamento de teclado, mouse e controle.")],
    ),
    (
        "Launcher options, diagnostics and system tools.",
        &[(
            "pt",
            "Opções do launcher, diagnóstico e ferramentas do sistema.",
        )],
    ),
    (
        "Output of the last start. last_run.log holds the same text.",
        &[(
            "pt",
            "Saída da última execução. last_run.log tem o mesmo texto.",
        )],
    ),
    // The word of the native render resolution's label, when the tables have no whole label.
    ("Native", &[("pt", "Nativa")]),
    // The Home saves chip (the Python's "• No saves yet: the game creates them in {}" and
    // "✓ Saves found in {}" without the folder) and the splash hint.
    ("• No saves yet", &[("pt", "• Nenhum save ainda")]),
    (
        "✓ Saves found",
        &[("pt", "✓ Saves encontrados"), ("es", "✓ Partidas guardadas")],
    ),
    // The Home hint line after a start that left patches out (ruling R85): the Python's
    // "{} not found in this eboot.bin at the last start (see Log)" with the word it followed.
    (
        "{} patches not found in this eboot.bin at the last start (see Log)",
        &[(
            "pt",
            "{} patches não encontrados neste eboot.bin na última inicialização (veja o Registro)",
        )],
    ),
    // The Home graphics card chip when the probe did not answer.
    (
        "• GPU: not checked",
        &[("pt", "• GPU: não verificada"), ("es", "• GPU: sin comprobar")],
    ),
    ("any key skips", &[("pt", "qualquer tecla pula")]),
    // The splash's two checks (ruling R97).
    (
        "Checking the settings…",
        &[("pt", "Verificando as configurações…")],
    ),
    (
        "Checking for updates…",
        &[("pt", "Verificando atualizações…")],
    ),
    (
        "Effects",
        &[("pt", "Efeitos"), ("es", "Efectos"), ("ru", "Эффекты")],
    ),
    (
        "Embers and flames on the Home screen. Off saves GPU time.",
        &[
            (
                "pt",
                "Brasas e chamas na tela inicial. Desligado economiza GPU.",
            ),
            (
                "es",
                "Brasas y llamas en la pantalla inicial. Apagado ahorra GPU.",
            ),
            (
                "ru",
                "Угли и пламя на главном экране. Выключено — экономия GPU.",
            ),
        ],
    ),
    (
        "Release",
        &[
            ("ar", "الإصدار"),
            ("es", "Lanzamiento"),
            ("pt", "Lançamento"),
            ("fr", "Version"),
            ("de", "Veröffentlichung"),
            ("it", "Versione"),
            ("pl", "Wydanie"),
            ("tr", "Sürüm"),
            ("zh", "正式版"),
            ("ja", "リリース"),
            ("ko", "릴리스"),
            ("id", "Rilis"),
            ("ru", "Релиз"),
        ],
    ),
    (
        "Pre-release",
        &[
            ("ar", "إصدار تجريبي"),
            ("es", "Prelanzamiento"),
            ("pt", "Pré-lançamento"),
            ("fr", "Préversion"),
            ("de", "Vorabversion"),
            ("it", "Versione preliminare"),
            ("pl", "Wersja przedpremierowa"),
            ("tr", "Ön sürüm"),
            ("zh", "预发布版"),
            ("ja", "プレリリース"),
            ("ko", "사전 릴리스"),
            ("id", "Pra-rilis"),
            ("ru", "Предрелиз"),
        ],
    ),
    (
        "Releases",
        &[
            ("ar", "الإصدارات"),
            ("es", "Lanzamientos"),
            ("pt", "Lançamentos"),
            ("fr", "Versions"),
            ("de", "Veröffentlichungen"),
            ("it", "Versioni"),
            ("pl", "Wydania"),
            ("tr", "Sürümler"),
            ("zh", "正式版"),
            ("ja", "リリース"),
            ("ko", "릴리스"),
            ("id", "Rilis"),
            ("ru", "Релизы"),
        ],
    ),
    (
        "Pre-releases",
        &[
            ("ar", "إصدارات تجريبية"),
            ("es", "Prelanzamientos"),
            ("pt", "Pré-lançamentos"),
            ("fr", "Préversions"),
            ("de", "Vorabversionen"),
            ("it", "Versioni preliminari"),
            ("pl", "Wersje przedpremierowe"),
            ("tr", "Ön sürümler"),
            ("zh", "预发布版"),
            ("ja", "プレリリース"),
            ("ko", "사전 릴리스"),
            ("id", "Pra-rilis"),
            ("ru", "Предрелизы"),
        ],
    ),
    (
        "new",
        &[
            ("ar", "جديد"),
            ("es", "nuevo"),
            ("pt", "novo"),
            ("fr", "nouveau"),
            ("de", "neu"),
            ("it", "nuovo"),
            ("pl", "nowe"),
            ("tr", "yeni"),
            ("zh", "新"),
            ("ja", "新着"),
            ("ko", "신규"),
            ("id", "baru"),
            ("ru", "новое"),
        ],
    ),
    (
        "{} available",
        &[
            ("ar", "{} متاح"),
            ("es", "{} disponible"),
            ("pt", "{} disponível"),
            ("fr", "{} disponible"),
            ("de", "{} verfügbar"),
            ("it", "{} disponibile"),
            ("pl", "{} dostępne"),
            ("tr", "{} mevcut"),
            ("zh", "{} 可用"),
            ("ja", "{} が利用可能"),
            ("ko", "{} 사용 가능"),
            ("id", "{} tersedia"),
            ("ru", "{} доступно"),
        ],
    ),
    (
        "Fetching…",
        &[
            ("ar", "جارٍ الجلب…"),
            ("es", "Obteniendo…"),
            ("pt", "Buscando…"),
            ("fr", "Récupération…"),
            ("de", "Wird abgerufen…"),
            ("it", "Recupero…"),
            ("pl", "Pobieranie…"),
            ("tr", "Alınıyor…"),
            ("zh", "正在获取…"),
            ("ja", "取得中…"),
            ("ko", "가져오는 중…"),
            ("id", "Mengambil…"),
            ("ru", "Загрузка…"),
        ],
    ),
    (
        "Could not fetch the versions",
        &[
            ("ar", "تعذّر جلب الإصدارات"),
            ("es", "No se pudieron obtener las versiones"),
            ("pt", "Não foi possível buscar as versões"),
            ("fr", "Impossible de récupérer les versions"),
            ("de", "Die Versionen konnten nicht abgerufen werden"),
            ("it", "Impossibile recuperare le versioni"),
            ("pl", "Nie udało się pobrać wersji"),
            ("tr", "Sürümler alınamadı"),
            ("zh", "无法获取版本"),
            ("ja", "バージョンを取得できませんでした"),
            ("ko", "버전을 가져올 수 없습니다"),
            ("id", "Gagal mengambil versi"),
            ("ru", "Не удалось получить версии"),
        ],
    ),
    (
        "No test builds",
        &[
            ("ar", "لا توجد إصدارات اختبارية"),
            ("es", "No hay compilaciones de prueba"),
            ("pt", "Sem builds de teste"),
            ("fr", "Aucune build de test"),
            ("de", "Keine Test-Builds"),
            ("it", "Nessuna build di test"),
            ("pl", "Brak wersji testowych"),
            ("tr", "Test sürümü yok"),
            ("zh", "没有测试版本"),
            ("ja", "テストビルドはありません"),
            ("ko", "테스트 빌드 없음"),
            ("id", "Tidak ada build uji"),
            ("ru", "Нет тестовых сборок"),
        ],
    ),
    (
        "No test builds for Linux",
        &[
            ("ar", "لا توجد إصدارات اختبارية لنظام Linux"),
            ("es", "No hay compilaciones de prueba para Linux"),
            ("pt", "Sem builds de teste para Linux"),
            ("fr", "Aucune build de test pour Linux"),
            ("de", "Keine Test-Builds für Linux"),
            ("it", "Nessuna build di test per Linux"),
            ("pl", "Brak wersji testowych dla Linuksa"),
            ("tr", "Linux için test sürümü yok"),
            ("zh", "没有适用于 Linux 的测试版本"),
            ("ja", "Linux 向けのテストビルドはありません"),
            ("ko", "Linux용 테스트 빌드 없음"),
            ("id", "Tidak ada build uji untuk Linux"),
            ("ru", "Нет тестовых сборок для Linux"),
        ],
    ),
    (
        "This version is older than the installed one.",
        &[
            ("ar", "هذا الإصدار أقدم من الإصدار المثبّت."),
            ("es", "Esta versión es anterior a la instalada."),
            ("pt", "Esta versão é mais antiga que a instalada."),
            ("fr", "Cette version est plus ancienne que celle installée."),
            ("de", "Diese Version ist älter als die installierte."),
            ("it", "Questa versione è più vecchia di quella installata."),
            ("pl", "Ta wersja jest starsza od zainstalowanej."),
            ("tr", "Bu sürüm, yüklü olandan eskidir."),
            ("zh", "此版本比已安装的版本旧。"),
            ("ja", "このバージョンはインストール済みのものより古いです。"),
            ("ko", "이 버전은 설치된 버전보다 오래되었습니다."),
            ("id", "Versi ini lebih lama dari yang terpasang."),
            ("ru", "Эта версия старше установленной."),
        ],
    ),
];

fn ui_extra(lang: &str, en: &str) -> Option<&'static str> {
    UI_EXTRA
        .iter()
        .find(|(key, _)| *key == en)?
        .1
        .iter()
        .find(|(code, _)| *code == lang)
        .map(|(_, text)| *text)
}

/// The text in `lang`; the English text when there is no translation.
pub fn tr(lang: &str, en: &str) -> String {
    if let Some(base) = en.strip_suffix(WARN) {
        return tr(lang, base) + WARN;
    }
    lookup(lang, en)
        .filter(|text| !text.is_empty())
        .or_else(|| ui_extra(lang, en).map(str::to_string))
        .unwrap_or_else(|| en.to_string())
}

/// Translates `en`, then replaces each `{}` in order with `args`; extra `{}` stay.
pub fn tr_fmt(lang: &str, en: &str, args: &[&str]) -> String {
    let text = tr(lang, en);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    let mut args = args.iter();
    while let Some(at) = rest.find("{}") {
        let Some(arg) = args.next() else { break };
        out.push_str(&rest[..at]);
        out.push_str(arg);
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// Name and description of trophy `id` (0-based); English when the language has no list.
pub fn trophy_text(lang: &str, id: usize) -> (String, String) {
    let texts = &table().trophies.texts;
    let list = texts
        .get(lang)
        .filter(|list| !list.is_empty())
        .or_else(|| texts.get("en"));
    list.and_then(|list| list.get(id))
        .cloned()
        .unwrap_or_default()
}

/// Text of the Trophies page in `lang`; English when there is none.
pub fn trophy_ui(lang: &str, en: &str) -> String {
    table()
        .trophies
        .ui
        .get(lang)
        .and_then(|ui| ui.get(en))
        .filter(|text| !text.is_empty())
        .cloned()
        .unwrap_or_else(|| en.to_string())
}

/// Launcher language for a Windows primary language id; `en` when unknown.
pub fn windows_language_code(primary: u32) -> &'static str {
    table()
        .windows_languages
        .get(&primary.to_string())
        .and_then(|code| LANGS.iter().copied().find(|lang| lang == code))
        .unwrap_or("en")
}

/// Language from `LC_ALL`, `LC_MESSAGES` or `LANG` (first two lowercase letters); `en` by default.
pub fn environment_language(env: &dyn Fn(&str) -> Option<String>) -> String {
    for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = env(name) {
            let mut chars = value.chars();
            if let (Some(a), Some(b)) = (chars.next(), chars.next()) {
                if a.is_ascii_lowercase() && b.is_ascii_lowercase() {
                    return format!("{a}{b}");
                }
            }
        }
    }
    "en".to_string()
}

/// The system language: the Windows UI language, or the environment elsewhere.
pub fn system_language() -> String {
    if cfg!(windows) {
        return match platform::user_default_ui_language() {
            Some(id) => windows_language_code(id & 0x3ff).to_string(),
            None => "en".to_string(),
        };
    }
    environment_language(&|name| std::env::var(name).ok())
}

/// The launcher's language: the `ui_language` setting, or the system language when it is empty
/// (the "System" choice). `system` is only asked then.
pub fn launcher_language(ui_language: &str, system: impl FnOnce() -> String) -> String {
    if ui_language.is_empty() {
        system()
    } else {
        ui_language.to_string()
    }
}

pub fn is_rtl(lang: &str) -> bool {
    lang == "ar"
}

pub fn region_of(lang: &str) -> Region {
    match lang {
        "fr" | "de" | "it" | "pl" | "ru" | "tr" => Region::Europe,
        "ar" | "zh" | "ja" | "ko" | "id" => Region::Asia,
        _ => Region::Americas,
    }
}

/// The language's own name; `English` for an unknown code.
pub fn native_name(lang: &str) -> &'static str {
    match lang {
        "ru" => "Русский",
        "ar" => "العربية",
        "es" => "Español",
        "pt" => "Português (Brasil)",
        "fr" => "Français",
        "de" => "Deutsch",
        "it" => "Italiano",
        "pl" => "Polski",
        "tr" => "Türkçe",
        "zh" => "简体中文",
        "ja" => "日本語",
        "ko" => "한국어",
        "id" => "Bahasa Indonesia",
        _ => "English",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_only_strings_have_the_spec_translations() {
        assert_eq!(tr("pt", "Americas"), "Américas");
        assert_eq!(tr("pt", "Asia & Middle East"), "Ásia e Oriente Médio");
        assert_eq!(tr("ru", "Europe"), "Европа");
        assert_eq!(tr("de", "Americas"), "Americas");
        // A key of the JSON table still comes from it.
        assert_eq!(tr("pt", "Graphics"), "Gráficos");
    }

    #[test]
    fn every_rust_only_string_has_portuguese() {
        // Ruling R33: each string of the Rust launcher alone comes in English and Portuguese.
        for (en, _) in UI_EXTRA {
            let pt = tr("pt", en);
            assert!(!pt.is_empty() && pt != *en, "no pt for {en:?}");
        }
    }

    #[test]
    fn no_rust_only_string_is_a_key_of_the_table() {
        // `tr` reads the JSON table first: such an entry would never show.
        let table = table();
        let shadowed: Vec<&str> = UI_EXTRA
            .iter()
            .map(|(en, _)| *en)
            .filter(|en| table.keys.iter().any(|key| key == en))
            .collect();
        assert!(shadowed.is_empty(), "in assets/i18n.json: {shadowed:#?}");
    }

    #[test]
    fn no_rust_only_translation_is_shadowed_by_the_table() {
        // `lookup` reads the JSON's extra texts and positional table (and the Russian pairs)
        // before `UI_EXTRA`: a translation it already finds would never show.
        let shadowed: Vec<(&str, &str)> = UI_EXTRA
            .iter()
            .flat_map(|(en, texts)| texts.iter().map(move |(lang, _)| (*lang, *en)))
            .filter(|(lang, en)| lookup(lang, en).is_some_and(|text| !text.is_empty()))
            .collect();
        assert!(shadowed.is_empty(), "in assets/i18n.json: {shadowed:#?}");
    }

    #[test]
    fn embedded_table_parses() {
        let table = table();
        assert_eq!(table.keys.len(), 154);
        assert_eq!(table.translations.len(), 12);
    }

    #[test]
    fn portuguese_translates_graphics() {
        assert_eq!(tr("pt", "Graphics"), "Gráficos");
    }

    #[test]
    fn russian_uses_the_inline_pairs() {
        assert_eq!(tr("ru", "Graphics"), "Графика");
    }

    #[test]
    fn unknown_text_falls_back_to_english() {
        assert_eq!(tr("pt", "No such text"), "No such text");
        assert_eq!(tr("ru", "No such text"), "No such text");
        assert_eq!(tr("xx", "Graphics"), "Graphics");
    }

    #[test]
    fn warn_suffix_survives_translation() {
        let text = tr("pt", "Graphics  ⚠");
        assert!(text.starts_with("Gráficos"));
        assert!(text.ends_with("  ⚠"));
    }

    #[test]
    fn every_language_table_is_complete() {
        let table = table();
        for (lang, texts) in &table.translations {
            assert_eq!(texts.len(), table.keys.len(), "{lang}");
        }
    }

    #[test]
    fn tr_fmt_fills_placeholders_in_order() {
        let both = tr_fmt("en", "{} · game version {}", &["A", "1.0"]);
        assert_eq!(both, "A · game version 1.0");
        let one = tr_fmt("en", "{} · game version {}", &["A"]);
        assert_eq!(one, "A · game version {}");
    }

    #[test]
    fn trophies_fall_back_to_english() {
        let (name, _) = trophy_text("en", 0);
        assert_eq!(name, "King of the Hill");
        assert_eq!(trophy_text("id", 0).0, name);
        assert_eq!(trophy_text("en", 35), (String::new(), String::new()));
        assert_eq!(trophy_ui("pt", "Trophies"), "Troféus");
        assert_eq!(trophy_ui("en", "Trophies"), "Trophies");
    }

    #[test]
    fn windows_language_0x16_is_pt() {
        assert_eq!(windows_language_code(0x16), "pt");
        assert_eq!(windows_language_code(0x19), "ru");
        assert_eq!(windows_language_code(0x3f), "en");
    }

    #[test]
    fn environment_language_reads_the_first_set_variable() {
        let env = |name: &str| (name == "LANG").then(|| "pt_BR.UTF-8".to_string());
        assert_eq!(environment_language(&env), "pt");
        assert_eq!(environment_language(&|_| None), "en");
        assert_eq!(environment_language(&|_| Some("C".to_string())), "en");
    }

    #[test]
    fn regions_cover_all_14() {
        let mut seen = Vec::new();
        for region in [Region::Americas, Region::Europe, Region::Asia] {
            for lang in LANGS.iter().filter(|l| region_of(l) == region) {
                seen.push(*lang);
            }
        }
        seen.sort_unstable();
        let mut all = LANGS.to_vec();
        all.sort_unstable();
        assert_eq!(seen, all);
        assert_eq!(region_of("en"), Region::Americas);
        assert_eq!(region_of("tr"), Region::Europe);
        assert_eq!(region_of("id"), Region::Asia);
        assert!(is_rtl("ar") && !is_rtl("en"));
        assert_eq!(native_name("pt"), "Português (Brasil)");
    }

    #[test]
    fn every_version_menu_string_has_every_language() {
        const KEYS: [&str; 11] = [
            "Release",
            "Pre-release",
            "Releases",
            "Pre-releases",
            "new",
            "{} available",
            "Fetching…",
            "Could not fetch the versions",
            "No test builds",
            "No test builds for Linux",
            "This version is older than the installed one.",
        ];
        for en in KEYS {
            for lang in [
                "ar", "es", "pt", "fr", "de", "it", "pl", "tr", "zh", "ja", "ko", "id", "ru",
            ] {
                let text = tr(lang, en);
                assert!(!text.is_empty() && text != en, "no {lang} for {en:?}");
            }
        }
        assert_eq!(tr_fmt("pt", "{} available", &["2.7.0"]), "2.7.0 disponível");
        assert_eq!(tr("pt", "Pre-release"), "Pré-lançamento");
    }
}
