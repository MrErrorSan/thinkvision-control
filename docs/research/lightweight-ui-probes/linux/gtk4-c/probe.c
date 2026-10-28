/* GTK 4 probe window: label, scale 0-100, switch, dropdown (3 items), button.
 * Build: cc -Os -o probe probe.c $(pkg-config --cflags --libs gtk4) && strip probe */
#include <gtk/gtk.h>

static void on_clicked(GtkButton *b, gpointer label) {
    gtk_label_set_text(GTK_LABEL(label), "clicked");
}

static void activate(GtkApplication *app, gpointer user_data) {
    GtkWidget *win = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(win), "probe");
    gtk_window_set_default_size(GTK_WINDOW(win), 600, 400);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_start(box, 16);
    gtk_widget_set_margin_end(box, 16);
    gtk_widget_set_margin_top(box, 16);
    gtk_widget_set_margin_bottom(box, 16);

    GtkWidget *label = gtk_label_new("Brightness");
    GtkWidget *scale = gtk_scale_new_with_range(GTK_ORIENTATION_HORIZONTAL, 0, 100, 1);
    gtk_range_set_value(GTK_RANGE(scale), 50);
    GtkWidget *sw = gtk_switch_new();
    gtk_widget_set_halign(sw, GTK_ALIGN_START);
    const char *items[] = {"Standard", "Movie", "Game", NULL};
    GtkWidget *dd = gtk_drop_down_new_from_strings(items);
    GtkWidget *btn = gtk_button_new_with_label("Apply");
    g_signal_connect(btn, "clicked", G_CALLBACK(on_clicked), label);

    gtk_box_append(GTK_BOX(box), label);
    gtk_box_append(GTK_BOX(box), scale);
    gtk_box_append(GTK_BOX(box), sw);
    gtk_box_append(GTK_BOX(box), dd);
    gtk_box_append(GTK_BOX(box), btn);
    gtk_window_set_child(GTK_WINDOW(win), box);
    gtk_window_present(GTK_WINDOW(win));
}

int main(int argc, char **argv) {
    GtkApplication *app = gtk_application_new("dev.probe.gtk4", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
