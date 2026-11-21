# سياسة Rust وDependencies

## MSRV

الحد الأدنى المدعوم (MSRV) هو **Rust 1.88.0**، وهو مثبت في `rust-toolchain.toml` ومعلن في `[workspace.package].rust-version`. يجب أن تمر جميع crates على MSRV المعلن؛ CI يستخدم الإصدار نفسه على Linux وWindows وmacOS. لا تُرفع النسخة إلا بقرار موثق وتحديث CI والـlockfile.

## التبعيات

تُفضّل مكتبات Rust القياسية ويُحافظ على صفر dependencies تشغيلية للكومبايلر وأداة CLI. الاعتماد الخارجي الحالي الوحيد هو `serde_json` للاختبارات كي تُحلل JSON فعلياً؛ لا يدخل في dependencies العادية لأي crate. أي dependency تشغيلية جديدة تحتاج مبرراً موثقاً ومراجعة ترخيص/نشاط/ثغرات، وفحص تأثيرها على MSRV وسطح الهجوم. يجب إبقاء `Cargo.lock` متتبّعاً واستخدام `--locked` في CI. لا تُقبل تحديثات dependency تلقائية دون مراجعة الاختبارات وسياسة الترخيص.

## بوابة التحديث

قبل merge: `cargo fmt --all -- --check`, `cargo test --locked --workspace`, و`cargo clippy --locked --workspace --all-targets -- -D warnings` على مصفوفة الأنظمة. أدوات fuzz ذات dependencies إضافية، إن أضيفت، توضع في workspace منفصل ولا تدخل build المنتج.
