program probe;
{ Lazarus/LCL probe window: label, trackbar 0-100, checkbox (LCL has no stock switch),
  combo (3 items), button. Form built in code (no .lfm). }
{$mode objfpc}{$H+}
uses
  {$IFDEF UNIX}cthreads,{$ENDIF}
  Interfaces, Classes, Forms, Controls, StdCtrls, ComCtrls;

type
  TProbeForm = class(TForm)
  private
    FLabel: TLabel;
    procedure ButtonClick(Sender: TObject);
  public
    constructor Create(AOwner: TComponent); override;
  end;

procedure TProbeForm.ButtonClick(Sender: TObject);
begin
  FLabel.Caption := 'clicked';
end;

constructor TProbeForm.Create(AOwner: TComponent);
var
  Track: TTrackBar;
  Check: TCheckBox;
  Combo: TComboBox;
  Btn: TButton;
begin
  inherited CreateNew(AOwner);
  Caption := 'probe';
  Width := 600;
  Height := 400;
  ChildSizing.Layout := cclLeftToRightThenTopToBottom;
  ChildSizing.ControlsPerLine := 1;
  ChildSizing.LeftRightSpacing := 16;
  ChildSizing.TopBottomSpacing := 16;
  ChildSizing.VerticalSpacing := 12;
  ChildSizing.EnlargeHorizontal := crsHomogenousChildResize;

  FLabel := TLabel.Create(Self);
  FLabel.Caption := 'Brightness';
  FLabel.Parent := Self;

  Track := TTrackBar.Create(Self);
  Track.Min := 0;
  Track.Max := 100;
  Track.Position := 50;
  Track.Parent := Self;

  Check := TCheckBox.Create(Self);
  Check.Caption := 'Enabled';
  Check.Parent := Self;

  Combo := TComboBox.Create(Self);
  Combo.Style := csDropDownList;
  Combo.Items.Add('Standard');
  Combo.Items.Add('Movie');
  Combo.Items.Add('Game');
  Combo.ItemIndex := 0;
  Combo.Parent := Self;

  Btn := TButton.Create(Self);
  Btn.Caption := 'Apply';
  Btn.OnClick := @ButtonClick;
  Btn.Parent := Self;
end;

var
  F: TProbeForm;
begin
  RequireDerivedFormResource := False;
  Application.Scaled := True;
  Application.Initialize;
  Application.CreateForm(TProbeForm, F);
  Application.Run;
end.
