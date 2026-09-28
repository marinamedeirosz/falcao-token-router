//! O `AutoRun` do Prompt de Comando: compor com quem já estava lá, e sair sem
//! levar ninguém junto.
//!
//! O `AutoRun` é um valor ÚNICO do usuário — clink, ConEmu e Anaconda põem o
//! deles ali. Sobrescrevê-lo quebra o terminal de quem usa essas ferramentas, e
//! o sintoma (um prompt que parou de funcionar) não tem como ser ligado à causa
//! (um botão "Ativar no cmd" clicado semanas antes). Por isso a suíte inteira
//! gira em torno de um terceiro presente.
//!
//! Nada aqui toca o `AutoRun` de verdade: cada teste usa uma subchave própria
//! sob `HKCU\Software\FalcaoRouterTests`, que se apaga sozinha no fim.

use router_core::platform::command_processor::{
    autorun, has_marker, install, remove, CommandProcessorKey, InstallOutcome, RemoveOutcome,
};
use windows_sys::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER};

/// O nosso: o `shell.cmd` que define a macro `doskey claude`.
const OUR_CALL: &str = r#""C:\Users\exemplo\AppData\Local\FalcaoRouter\shell.cmd""#;
const OUR_MARKER: &str = "FalcaoRouter";

/// O de terceiro: um clink qualquer, para ter alguém a preservar.
const CLINK_CALL: &str = r#""C:\Users\exemplo\clink\clink.bat" inject"#;
const CLINK_MARKER: &str = "clink";

/// A chave de teste, que se apaga sozinha — inclusive quando o teste falha.
struct TempKey(CommandProcessorKey);

impl TempKey {
    fn new(name: &str) -> Self {
        let key = CommandProcessorKey::for_tests(name);
        // Um teste anterior interrompido (Ctrl+C, travamento) pode ter deixado a
        // chave para trás; começar apagando torna cada teste independente.
        delete(&key);
        TempKey(key)
    }
}

impl Drop for TempKey {
    fn drop(&mut self) {
        delete(&self.0);
    }
}

/// Apaga SÓ a folha. A pasta-mãe é compartilhada com os outros testes, que o
/// cargo roda em paralelo neste mesmo binário: apagá-la derrubaria a chave que
/// outro teste tem aberta no meio de uma escrita.
fn delete(key: &CommandProcessorKey) {
    let subkey: Vec<u16> = key.subkey.encode_utf16().chain(Some(0)).collect();
    // SAFETY: a string UTF-16 termina em zero e vive até o fim da chamada; a
    // subchave é nossa, sob `Software\FalcaoRouterTests`.
    unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, subkey.as_ptr()) };
}

#[test]
fn install_composes_with_a_third_party_autorun() {
    let key = TempKey::new("compose");
    install(&key.0, CLINK_CALL, CLINK_MARKER).unwrap();

    assert_eq!(
        install(&key.0, OUR_CALL, OUR_MARKER).unwrap(),
        InstallOutcome::Added
    );
    assert_eq!(
        autorun(&key.0).as_deref(),
        Some(format!("{CLINK_CALL} & {OUR_CALL}").as_str())
    );
}

#[test]
fn install_on_an_empty_autorun_writes_only_ours() {
    let key = TempKey::new("empty");

    assert_eq!(
        install(&key.0, OUR_CALL, OUR_MARKER).unwrap(),
        InstallOutcome::Added
    );
    assert_eq!(autorun(&key.0).as_deref(), Some(OUR_CALL));
    assert!(has_marker(&key.0, OUR_MARKER));
}

#[test]
fn installing_twice_does_not_duplicate() {
    let key = TempKey::new("idempotent");
    install(&key.0, CLINK_CALL, CLINK_MARKER).unwrap();
    install(&key.0, OUR_CALL, OUR_MARKER).unwrap();
    let after_first = autorun(&key.0);

    assert_eq!(
        install(&key.0, OUR_CALL, OUR_MARKER).unwrap(),
        InstallOutcome::AlreadyPresent
    );
    assert_eq!(autorun(&key.0), after_first);
}

#[test]
fn remove_keeps_the_third_party_segment() {
    let key = TempKey::new("remove-keeps");
    install(&key.0, CLINK_CALL, CLINK_MARKER).unwrap();
    install(&key.0, OUR_CALL, OUR_MARKER).unwrap();

    assert_eq!(remove(&key.0, OUR_MARKER).unwrap(), RemoveOutcome::Removed);
    assert_eq!(autorun(&key.0).as_deref(), Some(CLINK_CALL));
    assert!(!has_marker(&key.0, OUR_MARKER));
}

/// O `&&` do terceiro é "só se o anterior deu certo"; devolvê-lo como `&` faria
/// o segundo comando dele passar a rodar sempre — uma mudança de comportamento
/// que ele não pediu e não veria.
#[test]
fn remove_preserves_the_third_party_separator() {
    let key = TempKey::new("remove-separator");
    let chained = format!("{CLINK_CALL} && echo pronto");
    install(&key.0, &chained, CLINK_MARKER).unwrap();
    install(&key.0, OUR_CALL, OUR_MARKER).unwrap();

    remove(&key.0, OUR_MARKER).unwrap();

    assert_eq!(autorun(&key.0).as_deref(), Some(chained.as_str()));
}

#[test]
fn remove_deletes_the_value_when_nothing_is_left() {
    let key = TempKey::new("remove-last");
    install(&key.0, OUR_CALL, OUR_MARKER).unwrap();

    assert_eq!(remove(&key.0, OUR_MARKER).unwrap(), RemoveOutcome::Removed);
    // Ausente, e não string vazia: um `AutoRun` vazio ainda é um `AutoRun`.
    assert_eq!(autorun(&key.0), None);
}

#[test]
fn remove_without_our_segment_is_not_present() {
    let key = TempKey::new("remove-absent");

    // Sem valor nenhum.
    assert_eq!(
        remove(&key.0, OUR_MARKER).unwrap(),
        RemoveOutcome::NotPresent
    );

    // Com valor, mas só de terceiro — que fica intacto.
    install(&key.0, CLINK_CALL, CLINK_MARKER).unwrap();
    assert_eq!(
        remove(&key.0, OUR_MARKER).unwrap(),
        RemoveOutcome::NotPresent
    );
    assert_eq!(autorun(&key.0).as_deref(), Some(CLINK_CALL));
}
