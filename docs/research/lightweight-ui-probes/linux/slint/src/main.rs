// Slint probe window: label, slider 0-100, switch, combo (3 items), button.
slint::slint! {
    import { Slider, Switch, ComboBox, Button, VerticalBox } from "std-widgets.slint";
    export component Probe inherits Window {
        title: "probe";
        preferred-width: 600px;
        preferred-height: 400px;
        in-out property <string> status: "Brightness";
        VerticalBox {
            alignment: start;
            Text { text: root.status; }
            Slider { minimum: 0; maximum: 100; value: 50; }
            Switch { text: "Enabled"; }
            ComboBox { model: ["Standard", "Movie", "Game"]; current-value: "Standard"; }
            Button { text: "Apply"; clicked => { root.status = "clicked"; } }
        }
    }
}

fn main() {
    Probe::new().unwrap().run().unwrap();
}
