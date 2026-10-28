// Gir.Core GTK 4 probe window: label, scale 0-100, switch, dropdown (3 items), button.
var app = Gtk.Application.New("dev.probe.gircore", Gio.ApplicationFlags.NonUnique);
app.OnActivate += (sender, _) =>
{
    var win = Gtk.ApplicationWindow.New((Gtk.Application)sender);
    win.Title = "probe";
    win.SetDefaultSize(600, 400);

    var box = Gtk.Box.New(Gtk.Orientation.Vertical, 12);
    box.MarginStart = box.MarginEnd = box.MarginTop = box.MarginBottom = 16;

    var label = Gtk.Label.New("Brightness");
    var scale = Gtk.Scale.NewWithRange(Gtk.Orientation.Horizontal, 0, 100, 1);
    scale.SetValue(50);
    var sw = Gtk.Switch.New();
    sw.Halign = Gtk.Align.Start;
    var dd = Gtk.DropDown.NewFromStrings(["Standard", "Movie", "Game"]);
    var btn = Gtk.Button.NewWithLabel("Apply");
    btn.OnClicked += (_, _) => label.SetText("clicked");

    box.Append(label);
    box.Append(scale);
    box.Append(sw);
    box.Append(dd);
    box.Append(btn);
    win.SetChild(box);
    win.Present();
};
return app.RunWithSynchronizationContext(null);
