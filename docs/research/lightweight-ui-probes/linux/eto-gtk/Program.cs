// Eto.Forms (GTK 3 backend via GtkSharp) probe window: label, slider 0-100,
// checkbox (no stock switch), dropdown (3 items), button.
using Eto.Drawing;
using Eto.Forms;

// Platform passed as an instance, not by name: the by-name lookup is reflection and the
// trimmer removes Eto.Gtk.dll entirely ("Platform type 'Eto.GtkSharp.Platform, Eto.Gtk' was not found").
var app = new Application(new Eto.GtkSharp.Platform());
var label = new Label { Text = "Brightness" };
var slider = new Slider { MinValue = 0, MaxValue = 100, Value = 50 };
var check = new CheckBox { Text = "Enabled" };
var combo = new DropDown();
combo.Items.Add("Standard");
combo.Items.Add("Movie");
combo.Items.Add("Game");
combo.SelectedIndex = 0;
var button = new Button { Text = "Apply" };
button.Click += (_, _) => label.Text = "clicked";

var form = new Form
{
    Title = "probe",
    ClientSize = new Size(600, 400),
    Content = new StackLayout
    {
        Padding = 16,
        Spacing = 12,
        HorizontalContentAlignment = HorizontalAlignment.Stretch,
        Items = { label, slider, check, combo, button },
    },
};
app.Run(form);
