// wxWidgets 3.2 (GTK3 port) probe window: label, slider 0-100, checkbox, choice (3 items), button.
// Build: g++ -Os -o probe probe.cpp $(wx-config --cxxflags --libs core,base) && strip probe
#include <wx/wx.h>

class ProbeFrame : public wxFrame {
public:
    ProbeFrame() : wxFrame(nullptr, wxID_ANY, "probe", wxDefaultPosition, wxSize(600, 400)) {
        auto *panel = new wxPanel(this);
        auto *sizer = new wxBoxSizer(wxVERTICAL);
        auto *label = new wxStaticText(panel, wxID_ANY, "Brightness");
        auto *slider = new wxSlider(panel, wxID_ANY, 50, 0, 100);
        auto *toggle = new wxCheckBox(panel, wxID_ANY, "Enabled");
        wxString items[] = {"Standard", "Movie", "Game"};
        auto *choice = new wxChoice(panel, wxID_ANY, wxDefaultPosition, wxDefaultSize, 3, items);
        choice->SetSelection(0);
        auto *button = new wxButton(panel, wxID_ANY, "Apply");
        button->Bind(wxEVT_BUTTON, [label](wxCommandEvent &) { label->SetLabel("clicked"); });
        for (wxWindow *w : {(wxWindow *)label, (wxWindow *)slider, (wxWindow *)toggle, (wxWindow *)choice, (wxWindow *)button})
            sizer->Add(w, 0, wxEXPAND | wxALL, 8);
        panel->SetSizer(sizer);
    }
};

class ProbeApp : public wxApp {
public:
    bool OnInit() override {
        (new ProbeFrame())->Show();
        return true;
    }
};

wxIMPLEMENT_APP(ProbeApp);
