//! 词库文件变化在配置不变时仍应进入正在运行的引擎。

use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use qingjian_core::Engine;
use qingjian_dictionary::{Dictionary, import};
use qingjian_platform::Config;

use super::CONFIG_POLL_INTERVAL;
use crate::dispatch::{DataDirs, Router, RouterConfig};

fn poll(router: &mut Router) {
    router.reload.as_mut().unwrap().last_check = Instant::now() - CONFIG_POLL_INTERVAL;
    router.poll_config_reload();
}

/// 拿一个词的译文：构造一条候选交给 `annotate`，看它挂上的 translation。
fn translation_of(router: &Router, word: &str) -> Option<String> {
    let mut list = qingjian_core::CandidateList {
        items: vec![qingjian_core::Candidate {
            text: word.to_owned(),
            kind: qingjian_core::CandidateKind::Chinese,
            syllables: Vec::new(),
            reading: None,
            translation: None,
            aux_code: None,
            settled: true,
        }],
    };
    router.engine.annotate(&mut list);
    list.items
        .first()
        .and_then(|candidate| candidate.translation.as_ref())
        .and_then(|translation| translation.senses().first())
        .map(|sense| sense.text.clone())
}

fn modified_at(path: &Path, seconds: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
}

#[test]
fn import_replace_and_remove_without_config_changes() {
    let dir = std::env::temp_dir().join(format!("qingjian-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.toml");
    std::fs::write(&config_path, "").unwrap();
    let config = Config::load(&config_path).unwrap();
    let original_config = std::fs::read(&config_path).unwrap();
    let mut router = Router::new(Engine::new(Dictionary::default()), RouterConfig::default());
    router.watch_config(
        &config,
        config_path.clone(),
        dir.clone(),
        DataDirs {
            user_root: Some(dir.clone()),
            user_dicts: Some(dir.join("dicts")),
            ..DataDirs::default()
        },
    );

    let source = dir.join("law.dict.yaml");
    std::fs::write(&source, "---\nname: law\n...\n合同法\the tong fa\t120\n").unwrap();
    let imported = import::import(&source, &dir.join("dicts")).unwrap();
    modified_at(&imported.path, 100);
    poll(&mut router);
    let dictionaries = router.engine.extra_dictionaries();
    assert_eq!(dictionaries.len(), 1);
    assert_eq!(
        dictionaries[0].lookup(&["he", "tong", "fa"], false)[0].text,
        "合同法"
    );

    // 只改词频，输出长度保持不变，必须靠 mtime 发现更新。
    let original_len = std::fs::metadata(&imported.path).unwrap().len();
    std::fs::write(&source, "---\nname: law\n...\n合同法\the tong fa\t121\n").unwrap();
    import::import(&source, &dir.join("dicts")).unwrap();
    modified_at(&imported.path, 200);
    assert_eq!(
        std::fs::metadata(&imported.path).unwrap().len(),
        original_len
    );
    poll(&mut router);
    assert_eq!(
        router.engine.extra_dictionaries()[0].lookup(&["he", "tong", "fa"], false)[0].frequency,
        121
    );

    // 保持旧词库映射打开，模拟运行中的 Server 收到同名替换。
    std::fs::write(
        &source,
        "---\nname: law\n...\n民法典\tmin fa dian\t100\n法律\tfa lv\t30\n",
    )
    .unwrap();
    import::import(&source, &dir.join("dicts")).unwrap();
    poll(&mut router);
    let dictionary = &router.engine.extra_dictionaries()[0];
    assert!(dictionary.lookup(&["he", "tong", "fa"], false).is_empty());
    assert_eq!(
        dictionary.lookup(&["min", "fa", "dian"], false)[0].text,
        "民法典"
    );

    // YAML 头中只有引用、没有本文件词条时，失败不能覆盖已有词库。
    std::fs::write(&source, "---\nname: law\nimport_tables:\n  - other\n...\n").unwrap();
    assert!(import::import(&source, &dir.join("dicts")).is_err());
    assert_eq!(Dictionary::from_path(&imported.path).unwrap().len(), 2);

    let removed = dir.join("dicts/removed");
    std::fs::create_dir_all(&removed).unwrap();
    std::fs::rename(&imported.path, removed.join("law.qj")).unwrap();
    poll(&mut router);
    assert!(router.engine.extra_dictionaries().is_empty());
    assert_eq!(std::fs::read(&config_path).unwrap(), original_config);
    drop(router);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn dictionary_changes_do_not_retry_broken_config() {
    let dir = std::env::temp_dir().join(format!("qingjian-reload-broken-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.toml");
    std::fs::write(&config_path, "[general]\npage_size = 5\n").unwrap();
    modified_at(&config_path, 100);
    let config = Config::load(&config_path).unwrap();
    let mut router = Router::new(
        Engine::new(Dictionary::default()),
        RouterConfig::from(&config),
    );
    router.watch_config(
        &config,
        config_path.clone(),
        dir.clone(),
        DataDirs {
            user_root: Some(dir.clone()),
            user_dicts: Some(dir.join("dicts")),
            ..DataDirs::default()
        },
    );

    std::fs::write(&config_path, "[broken").unwrap();
    modified_at(&config_path, 200);
    poll(&mut router);
    let source = dir.join("law.dict.yaml");
    std::fs::write(&source, "---\nname: law\n...\n合同法\the tong fa\t120\n").unwrap();
    let imported = import::import(&source, &dir.join("dicts")).unwrap();
    poll(&mut router);
    assert_eq!(router.config.page_size, 5);
    assert_eq!(router.engine.extra_dictionaries().len(), 1);

    // 保持坏配置的 mtime，以可观察的配置值确认后续轮询不会重新读它。
    std::fs::write(&config_path, "[general]\npage_size = 9\n").unwrap();
    modified_at(&config_path, 200);
    poll(&mut router);
    assert_eq!(router.config.page_size, 5);
    let removed = dir.join("dicts/removed");
    std::fs::create_dir_all(&removed).unwrap();
    std::fs::rename(&imported.path, removed.join("law.qj")).unwrap();
    poll(&mut router);
    assert_eq!(router.config.page_size, 5);
    assert!(router.engine.extra_dictionaries().is_empty());

    modified_at(&config_path, 300);
    poll(&mut router);
    assert_eq!(router.config.page_size, 9);
    drop(router);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn term_table_and_personal_glossary_reload_without_restart() {
    let dir = std::env::temp_dir().join(format!("qingjian-reload-terms-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.toml");
    // 学习语言显式写英文：personal glossary 要按语言找文件
    std::fs::write(&config_path, "[general]\nlearning_language = \"en\"\n").unwrap();
    let config = Config::load(&config_path).unwrap();

    let mut router = Router::new(Engine::new(Dictionary::default()), RouterConfig::default());
    router.watch_config(
        &config,
        config_path.clone(),
        dir.clone(),
        DataDirs {
            user_root: Some(dir.clone()),
            user_dicts: Some(dir.join("dicts")),
            ..DataDirs::default()
        },
    );

    // 随包英文释义表放在 `<root>/assets/glossary/`（与 `assembly::glossary_file` 一致）；
    // 这些文件在 watch_config 之后出现，第一次 poll 就会当成「数据变了」，装释义表与附加词库。
    let glossary_dir = dir.join("assets/glossary");
    std::fs::create_dir_all(&glossary_dir).unwrap();
    std::fs::write(glossary_dir.join("glossary-en.tsv"), "围挡\tbarricade\n").unwrap();
    std::fs::write(
        dir.join("user-terms.tsv"),
        "围挡\twei dang\t工程施工\tY\tbarricade\n",
    )
    .unwrap();
    std::fs::write(dir.join("user-glossary-en.tsv"), "围挡\tbarricade\n").unwrap();
    poll(&mut router);
    assert_eq!(
        translation_of(&router, "围挡"),
        Some("barricade".to_owned())
    );

    // 用户在设置里把译名改成 hoarding：术语表与个人释义表都会被重写
    std::fs::write(
        dir.join("user-terms.tsv"),
        "围挡\twei dang\t工程施工\tY\thoarding\n",
    )
    .unwrap();
    std::fs::write(dir.join("user-glossary-en.tsv"), "围挡\thoarding\n").unwrap();
    poll(&mut router);
    assert_eq!(
        translation_of(&router, "围挡"),
        Some("hoarding".to_owned()),
        "改了术语译名，不重启也该读到新的"
    );

    // 新加一条术语：附加词库当场重装，能查着
    std::fs::write(
        dir.join("user-terms.tsv"),
        "围挡\twei dang\t工程施工\tY\thoarding\n总包\tzong bao\t工程施工\tY\tmain contractor\n",
    )
    .unwrap();
    poll(&mut router);
    let dictionaries = router.engine.extra_dictionaries();
    assert!(
        dictionaries
            .iter()
            .any(|dictionary| !dictionary.lookup(&["zong", "bao"], false).is_empty()),
        "新加的术语应该不用重启就进词库"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn term_words_survive_settings_rebuild() {
    use std::collections::HashSet;
    use std::sync::Arc;

    let mut router = Router::new(Engine::new(Dictionary::default()), RouterConfig::default());
    let words: Arc<HashSet<String>> = Arc::new(["围挡".to_owned()].into_iter().collect());
    router.set_term_words(words.clone());
    // 热加载里重建画法（主题戳变化 / apply_config）都要带着术语集合
    assert!(router.render_settings().term_words.contains("围挡"));
}
