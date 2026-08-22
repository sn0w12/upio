use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressProps};
use tw_merge::*;

#[component]
pub fn Progress(props: ProgressProps) -> Element {
    // The primitive sets `--progress-value` (0–100%) on the root and emits
    // `data-state` ("complete"/"indeterminate") plus value/max attributes.
    let (caller_class, attributes) = crate::components::ui::split_class(props.attributes);

    rsx! {
        progress::Progress {
            class: tw_merge!("h-1.5 w-full overflow-hidden rounded-full bg-input", caller_class,),
            value: props.value,
            max: props.max,
            attributes,
            progress::ProgressIndicator { class: "bg-primary w-[var(--progress-value)] h-full transition-all ease-snappy duration-500" }
        }
    }
}
