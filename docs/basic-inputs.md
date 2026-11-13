# Slider, stepper, tooltip, and toast

`rndr_ui::inputs` adds four controlled controls on the existing input, focus, and accessibility path. Mount publishes a value. Keys, drags, step buttons, and assistive set-value send a request and leave that published value in place until the host mounts again. A later mount with the same value is the rejection boundary. A later mount with a different value replaces it.

These checks are headless. They do not certify a platform screen reader, a physical keyboard, or reduced-motion preferences read from a real desktop session. `SystemAppearance::reduced_motion` and `MotionSettings::reduced_motion` are honored when a test or host publishes them.

## Slider and range

`Slider` is one value. `Range` is two thumbs that cannot cross. Both use `SliderProps`: a nonblank label, an optional description, enabled and readonly, a `NumberRange`, and a vertical flag. The axis is fixed for the life of that id; changing `vertical` on an existing control returns `InvalidProps`.

`NumberRange` requires a finite `min < max`, a positive `step`, and a positive `page`. Mount clamps and snaps the initial value. Endpoints stay exact, so Home and End can land on a max that is not on the step grid.

Keyboard, on the focused slider or thumb:

| Key | Effect |
| --- | --- |
| Right, Up | Increase by `step` |
| Left, Down | Decrease by `step` |
| Shift+arrow, Page Up, Page Down | Change by `page` |
| Home, End | Minimum or maximum for that thumb |

A range End on the start thumb stops at the end thumb, and Home on the end thumb stops at the start thumb. A pointer press focuses the nearest thumb and requests the value under the pointer. Further movement tracks that thumb until release.

`SliderChange` and `RangeChange` carry the complete desired value. The thumb and `NumericAccess` keep the last mounted value. Assistive Increment, Decrement, and SetValue become the same requests through `NumericAccessSet`. A rejected string returns `Rejected` and writes nothing. Readonly focuses but does not change. Disabled is not in the tab order.

## Number field

`NumberField` is a spin button between decrease and increase buttons. Up and Down step. Shift+arrow, Page Up, and Page Down use `page`. Home and End jump to the numeric ends. Left, Right, Backspace, and typed characters edit the draft. Enter, or leaving the field, parses the draft. A number is clamped and snapped and sent as `NumberChange`. Anything else sends `NumberRejected` and restores the last requested text. The field shows the draft immediately so typing is visible. `NumericAccess.value` stays on the last mount until the host mounts again.

The decrease and increase buttons are named `Decrease {label}` and `Increase {label}`. They use the same step as the arrows.

## Tooltip

`Tooltip::attach` puts one passive popup on a target. Hovering the target or focusing it shows the tooltip. Pointer leave, blur, and Escape hide it. Escape while the target is focused keeps it hidden until focus and the pointer both leave. The popup uses `AccessRole::Tooltip`. `DescribedBy` on the target includes it; the accessibility bridge omits the link while the popup is hidden. The tooltip does not take focus and does not animate.

## Toast

`Toast::mount` places a status region in the parent the host supplies. It does not add a second overlay stack. `ToastProps::duration` of zero stays until Dismiss, Escape, or `ToastHandle::dismiss`. A positive duration expires while the pointer is outside and focus is outside. Hovering or focusing pauses the remaining time. `undo: Some(label)` adds an undo button; activating it sends `ToastNotice::Undo` and closes the toast.

The root is `AccessRole::Status` with a polite `LiveRegion`. The message is the accessible name. Dismiss and Undo are ordinary buttons. With reduced motion the toast appears and disappears on the notice frame. Otherwise it fades in and, on close, fades out before it is hidden. The notice is sent when closing starts.

Remounting the same id with a new message shows the toast again and restarts the timer.

## Not in this increment

Date pickers, date-range pickers, and color pickers are not implemented. They need a calendar model and a color space that this pass does not add.

## Run

```sh
cargo test --offline --locked -p rndr_ui --test inputs
cargo test --offline --locked -p rndr_render --lib slider_spin_button_and_status_reach_accesskit
cargo run -p rndr_ui --example inputs_gallery
```
