//! `router uninstall-integration` — o que ele tira, o que ele deixa, e o que
//! ele faz quando não há nada para tirar.
//!
//! O que importa aqui não é "a linha sumiu": é **o que ficou**. Quem desinstala
//! o app não pode perder o `.bashrc` junto — o modo de falha do macOS (perfil
//! ilegível virava "vazio" e era reescrito só com a linha nova) custou o arquivo
//! inteiro de alguém. Por isso todo teste põe conteúdo do usuário em volta e
//! confere que ele sobreviveu.
//!
//! Nada aqui toca o `AutoRun` de verdade: `ROUTER_CMD_TEST_KEY` manda o comando
//! para uma subchave sob `HKCU\Software\FalcaoRouterTests`, que se apaga no fim.
//! Os `$PROFILE` do PowerShell também não: eles saem da pasta Documentos REAL
//! (`SHGetKnownFolderPath`, que nenhuma variável de ambiente desvia), e o
//! marcador que o comando procura é a linha do sandbox — que não está lá. O
//! primeiro teste confere isso pela saída, em vez de adivinhar onde a Documentos
//! desta máquina mora.

mod common;
use common::*;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use router_core::engine::shell_integration::ShellIntegration;
use router_core::platform::command_processor::{autorun, install, CommandProcessorKey};

/// O de terceiro: um clink qualquer, para ter alguém a preservar no `AutoRun`.
const CLINK_CALL: &str = r#""C:\Users\exemplo\clink\clink.bat" inject"#;
const CLINK_MARKER: &str = "clink";

/// O nosso segmento. A forma exata é da trilha do botão "Ativar no cmd"; o que
/// esta suíte pina é o contrato de que a remoção depende: UM segmento, com o
/// marcador que o quadro da integração procura dentro dele.
const OUR_CALL: &str = r#"call "C:\Users\exemplo\shell.cmd" rem falcao-router-shim"#;
const OUR_MARKER: &str = "falcao-router-shim";

/// A chave de teste, que se apaga sozinha — inclusive quando o teste falha.
/// Sem `windows-sys` aqui (não é dependência do `router-cli`), quem apaga é o
/// `reg.exe`, que roda com o ambiente real, fora do sandbox.
struct TempKey {
    key: CommandProcessorKey,
    name: String,
}

impl TempKey {
    fn new(name: &str) -> Self {
        let key = TempKey {
            key: CommandProcessorKey::for_tests(name),
            name: name.to_string(),
        };
        key.delete(); // um teste interrompido antes pode ter deixado sobra
        key
    }

    fn delete(&self) {
        let _ = Command::new("reg")
            .args([
                "delete",
                &format!(r"HKCU\Software\FalcaoRouterTests\{}", self.name),
                "/f",
            ])
            .output();
    }
}

impl Drop for TempKey {
    fn drop(&mut self) {
        self.delete();
    }
}

/// Instala a integração no sandbox: os scripts e a linha do `~/.bashrc`, com
/// conteúdo do usuário antes e depois dela.
fn install_in_sandbox(sandbox: &Sandbox) -> PathBuf {
    let base = sandbox.paths().base;
    fs::create_dir_all(&base).unwrap();
    let (ps1, sh, cmd) = (
        base.join("shell.ps1"),
        base.join("shell.sh"),
        base.join("shell.cmd"),
    );
    ShellIntegration::write_scripts(&assert_cmd::cargo::cargo_bin("router"), &ps1, &sh, &cmd)
        .unwrap();

    let bashrc = sandbox.home.join(".bashrc");
    fs::write(&bashrc, "export MEU=1\nalias ll='ls -la'\n").unwrap();
    ShellIntegration::ensure_in_profile(&bashrc, &ShellIntegration::bash_source_line(&sh), "\n")
        .unwrap();
    let with_block = fs::read_to_string(&bashrc).unwrap();
    fs::write(&bashrc, format!("{with_block}# depois\nexport DEPOIS=1\n")).unwrap();
    bashrc
}

fn uninstall(sandbox: &Sandbox, key: &TempKey) -> Output {
    sandbox
        .router(&["uninstall-integration"])
        .env("ROUTER_CMD_TEST_KEY", &key.name)
        .output()
        .unwrap()
}

/// O caminho de uma peça inteira: `.bashrc` e `AutoRun` limpos, o que era do
/// usuário (e do clink) intacto.
#[test]
fn it_removes_our_line_and_our_autorun_segment_and_keeps_the_rest() {
    let sandbox = Sandbox::new();
    let key = TempKey::new("uninstall_removes");
    let bashrc = install_in_sandbox(&sandbox);
    install(&key.key, CLINK_CALL, CLINK_MARKER).unwrap();
    install(&key.key, OUR_CALL, OUR_MARKER).unwrap();
    assert!(
        fs::read_to_string(&bashrc).unwrap().contains("shell.sh"),
        "a linha nem chegou a ser instalada"
    );

    let out = uninstall(&sandbox, &key);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}\n{}", stderr(&out));

    let after = fs::read_to_string(&bashrc).unwrap();
    assert!(!after.contains("shell.sh"), "a linha ficou: {after:?}");
    assert!(!after.to_lowercase().contains("falcao"), "{after:?}");
    for kept in ["export MEU=1", "alias ll='ls -la'", "export DEPOIS=1"] {
        assert!(after.contains(kept), "sumiu {kept:?} do perfil: {after:?}");
    }

    // O clink fica, e sem um `&` solto na frente (sintaxe inválida no cmd).
    let value = autorun(&key.key).unwrap();
    assert!(!value.to_lowercase().contains(OUR_MARKER), "{value}");
    assert!(value.contains("clink.bat"), "{value}");
    assert!(!value.trim_start().starts_with('&'), "{value}");

    // Falante: cada peça é nomeada na saída.
    assert!(text.contains("AutoRun"), "{text}");
    assert!(text.contains(".bashrc"), "{text}");
    // E só o sandbox foi mexido: o `$PROFILE` de quem roda a suíte sai da
    // Documentos real, e nele não há o que remover.
    for line in text.lines().filter(|l| l.starts_with("  ok  ")) {
        assert!(
            line.contains(".bashrc") || line.contains("AutoRun"),
            "mexeu fora do sandbox: {line}"
        );
    }
}

/// Desinstalar duas vezes tem de dar certo as duas: o que não está lá não é
/// erro. (O gancho do desinstalador roda sem saber se o app já foi limpo.)
#[test]
fn removing_twice_is_not_an_error() {
    let sandbox = Sandbox::new();
    let key = TempKey::new("uninstall_twice");
    let bashrc = install_in_sandbox(&sandbox);
    install(&key.key, OUR_CALL, OUR_MARKER).unwrap();

    assert!(uninstall(&sandbox, &key).status.success());
    let out = uninstall(&sandbox, &key);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}\n{}", stderr(&out));
    assert!(text.contains("  --  "), "nada a remover não é erro: {text}");
    assert!(!text.contains("  !!  "), "{text}");

    // Sem segmento nenhum, o valor SAI — um `AutoRun` vazio ainda é um AutoRun,
    // e o cmd reclama dele a cada prompt.
    assert_eq!(autorun(&key.key), None);
    assert!(fs::read_to_string(&bashrc)
        .unwrap()
        .contains("export MEU=1"));
}

/// Perfil que nunca teve a linha não é tocado — nem no conteúdo nem na hora de
/// modificação. É a garantia de que rodar isto por engano não custa nada.
#[test]
fn a_profile_without_our_line_is_left_alone() {
    let sandbox = Sandbox::new();
    let key = TempKey::new("uninstall_untouched");
    let bashrc = sandbox.home.join(".bashrc");
    let mine = "# só do usuário\nexport MEU=1\n";
    fs::write(&bashrc, mine).unwrap();
    let before = fs::metadata(&bashrc).unwrap().modified().unwrap();

    assert!(uninstall(&sandbox, &key).status.success());

    assert_eq!(fs::read_to_string(&bashrc).unwrap(), mine);
    assert_eq!(fs::metadata(&bashrc).unwrap().modified().unwrap(), before);
}
