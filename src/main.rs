mod helpers;
use closure::closure;
use helpers::SafeHtml;
use melodeon::context::CtxErr;
use melorun::Runner;
use std::path::Path;
use web_sys::{HtmlInputElement, HtmlTextAreaElement};
use yew::prelude::*;
use yew_hooks::use_debounce;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// Compresses a string (smaz => lz4)
fn crush(code: &str, ctx: &str) -> String {
    let to_crush = serde_yaml::to_string(&[code, ctx]).unwrap();
    let c = &lz4_flex::compress_prepend_size(&smaz::compress(to_crush.as_bytes()));
    base64::encode_config(&c, base64::URL_SAFE_NO_PAD)
}

/// Decompresses a string
fn uncrush(s: &str) -> Option<(String, String)> {
    log::debug!("uncrushing {}", s);
    let decoded = base64::decode_config(&s, base64::URL_SAFE_NO_PAD).ok()?;
    let decompressed =
        smaz::decompress(&lz4_flex::decompress_size_prepended(&decoded).ok()?).ok()?;
    serde_yaml::from_str(&String::from_utf8_lossy(&decompressed)).ok()
}

#[function_component(HelloWorld)]
fn hello_world() -> Html {
    // The content of the code
    let code_string = use_state_eq(|| {
        web_sys::window()
            .unwrap()
            .location()
            .hash()
            .ok()
            .and_then(|s| uncrush(s.trim_matches('#')))
            .unwrap_or_default()
            .0
    });
    // The content of the context file
    let context_string = use_state_eq(|| {
        web_sys::window()
            .unwrap()
            .location()
            .hash()
            .ok()
            .and_then(|s| uncrush(s.trim_matches('#')))
            .unwrap_or_default()
            .1
    });
    // The content of the REPL interaction
    let repl_string = use_state_eq(String::new);
    // debounced function for updating the #... part of the URL
    let update_url = {
        let code_string = code_string.clone();
        let context_string = context_string.clone();
        use_debounce(
            move || {
                let hehe = crush(&code_string, &context_string);
                log::debug!("c: {}", hehe);
                let _ = web_sys::window().unwrap().location().set_hash(&hehe);
            },
            200,
        )
    };
    // error message, in RAW HTML!
    let error_message: UseStateHandle<Option<String>> = use_state_eq(|| None);
    // vector of past REPL interactions
    let first_res: UseStateHandle<Option<(String, String)>> = use_state_eq(|| None);
    let past_interactions: UseStateHandle<Vec<(String, (String, String))>> = use_state(Vec::new);
    let runner = use_mut_ref(|| melorun::Runner::new(None));
    let handle_err = closure!(clone error_message, clone code_string, |err: CtxErr| {
        error_message.set(
            ansi_to_html::convert_escaped(&err.pretty_print(|_| code_string.to_string().into()))
                .ok()
                .map(|s| s.replace('\n', "<br/>").replace("color:#5f5", "color:#262")),
        )
    });
    // on input of
    let on_code_input = {
        let code_string = code_string.clone();
        let update_url = update_url.clone();
        move |e: InputEvent| {
            let tgt: HtmlTextAreaElement = e.target_dyn_into().unwrap();
            let s = tgt.value();
            code_string.set(s);
            update_url.run();
        }
    };
    let on_ctx_input = {
        let context_string = context_string.clone();
        let update_url = update_url.clone();
        move |e: InputEvent| {
            let tgt: HtmlTextAreaElement = e.target_dyn_into().unwrap();
            let s = tgt.value();
            context_string.set(s);
            update_url.run();
        }
    };
    let on_repl_input = {
        let repl_string = repl_string.clone();
        move |e: InputEvent| {
            let tgt: HtmlInputElement = e.target_dyn_into().unwrap();
            let s = tgt.value();
            repl_string.set(s);
        }
    };
    let on_repl_keyup = {
        let repl_string = repl_string.clone();
        let runner = runner.clone();
        let error_message = error_message.clone();
        let past_interactions = past_interactions.clone();
        move |e: KeyboardEvent| {
            if e.key_code() == 13 {
                log::debug!("executing repl {:?}", repl_string.as_str());
                match runner.borrow_mut().run_repl_line(&repl_string) {
                    Ok((v, t)) => {
                        let mut pi = past_interactions.to_vec();
                        pi.push((
                            repl_string.to_string(),
                            (melorun::mvm_pretty(&v), format!("{:?}", t)),
                        ));
                        past_interactions.set(pi);
                        error_message.set(None);
                        repl_string.set("".into());
                    }
                    Err(err) => error_message.set(err.to_string().into()),
                }
            }
        }
    };
    // On clicking the "run" button
    let on_run = {
        let handle_err = handle_err;
        let code_string = code_string.clone();
        let context_string = context_string.clone();
        let first_res = first_res.clone();
        let error_message = error_message.clone();
        let past_interactions = past_interactions.clone();
        move || {
            past_interactions.set(vec![]);
            error_message.set(None);
            first_res.set(None);
            let code = code_string.as_str();
            let runner = runner.clone();
            let first_res = first_res.clone();
            log::debug!("compiling {}", code);
            let mut runner = runner.borrow_mut();
            if !context_string.is_empty() {
                *runner = Runner::new(Some(match serde_yaml::from_str(&context_string) {
                    Ok(res) => res,
                    Err(err) => {
                        error_message.set(Some(err.to_string()));
                        return;
                    }
                }))
            }
            match runner.load_str(Path::new("."), code) {
                Ok((result, t, _)) => {
                    log::debug!("result: {:?}", result);
                    first_res.set(Some((melorun::mvm_pretty(&result), format!("{:?}", t))));
                }
                Err(melorun::LoadFileError::MeloError(err)) => handle_err(err),
                Err(err) => error_message.set(Some(err.to_string())),
            }
        }
    };

    let _eff = {
        let on_run = on_run.clone();
        let error_message = error_message.clone();
        use_effect_with_deps(
            move |_| {
                on_run();
                error_message.set(None);
                || {}
            },
            (),
        )
    };

    html! {
        <div class="wrapper">
            <div class="pane">
                <div class="button-row">
                    <button onclick={move |_| on_run()}>{"Run"}</button>
                    <button onclick={closure!(clone code_string, clone update_url, clone past_interactions, clone first_res, clone error_message,
                         |_| {code_string.set("".into());
                         past_interactions.set(vec![]);
                         first_res.set(None);
                         error_message.set(None);
                         code_string.set("".into()); update_url.run()})}>{"Clear"}</button>
                </div>
                <textarea class="codearea" value={code_string.to_string()} oninput={on_code_input}></textarea>
            </div>
            <div class="pane">
                <div class="env-pane">
                    <small>{"SPEND ENVIRONMENT"}</small>
                    <textarea value={context_string.to_string()} oninput={on_ctx_input}></textarea>
                </div>
                if let Some((v, t)) = first_res.as_ref() {
                    <small>{"PROGRAM OUTPUT"}</small>
                    <div class="repl-row">
                        <div class="repl-type">
                            {"- : "} { t }
                        </div>
                        <div class="repl-result">
                            { v }
                        </div>
                    </div>
                }
                <div class="repl-row" key="fix" id="uniqqq">
                    <div class="repl-prompt">
                        <div>{"> "}</div>
                        <input class="repl-input" oninput={on_repl_input} onkeyup={on_repl_keyup} value={repl_string.to_string()} key="__un" autofocus={true}/>
                        <button>{"↩️"}</button>
                    </div>
                    if let Some(err) = error_message.as_ref() {
                        <div class="repl-error">
                            <SafeHtml html={err.to_string()} />
                        </div>
                    }
                </div>
                {for past_interactions.iter().rev().enumerate().map(|(i, (input, (v, t)))| {
                    html! {
                        <div class="repl-row" key={i.to_string()}>
                            <div class="repl-prompt">
                                <div>{"> "}</div>
                                <input class="repl-input" value={input.to_string()} disabled=true/>
                            </div>
                            <div class="repl-type">
                                {"- : "} { t }
                            </div>
                            <div class="repl-result">
                                { v }
                            </div>
                        </div>
                    }
                })}
            </div>
        </div>
    }
}

fn main() {
    colored::control::SHOULD_COLORIZE.set_override(true);
    wasm_logger::init(wasm_logger::Config::default());
    yew::start_app::<HelloWorld>();
}
