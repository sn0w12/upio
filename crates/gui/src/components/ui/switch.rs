use dioxus::prelude::*;
use dioxus_primitives::switch::{self, SwitchProps};
use tw_merge::*;

#[component]
pub fn Switch(props: SwitchProps) -> Element {
    // The primitive emits `data-state` ("checked"/"unchecked") and
    // `data-disabled` ("true"/"false"). A caller class arrives inside
    // `attributes`; merge it instead of letting the primitive's internal
    // attribute spread replace our base classes.
    let (caller_class, attributes) = crate::components::ui::split_class(props.attributes);
    let merged_class = tw_merge!(
        "inline-flex h-[calc(var(--thumb-size)+2px)] w-[calc(var(--thumb-size)*2-2px)] shrink-0 items-center rounded-full p-px outline-none transition-[background-color,box-shadow] duration-200 [--thumb-size:--spacing(5)] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background data-[state=checked]:bg-primary data-[state=unchecked]:bg-input data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-64 sm:[--thumb-size:--spacing(4)] group",
        caller_class,
    );

    rsx! {
        switch::Switch {
            class: merged_class,
            checked: props.checked,
            default_checked: props.default_checked,
            disabled: props.disabled,
            required: props.required,
            name: props.name,
            value: props.value,
            on_checked_change: props.on_checked_change,
            attributes,
            switch::SwitchThumb { class: "pointer-events-none block aspect-square h-full origin-left in-[[role=switch]:active,[data-slot=label]:active,[data-slot=field-label]:active]:scale-x-110 in-[[role=switch]:active,[data-slot=label]:active,[data-slot=field-label]:active]:rounded-[var(--thumb-size)/calc(var(--thumb-size)*1.1)] rounded-(--thumb-size) bg-background shadow-sm/5 will-change-transform [transition:translate_.15s,border-radius_.15s,scale_.1s_.1s,transform-origin_.15s] group-data-[state=checked]:origin-[var(--thumb-size)_50%] group-data-[state=checked]:translate-x-[calc(var(--thumb-size)-4px)]" }
        }
    }
}
