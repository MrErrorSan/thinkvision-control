using System.Collections;
using System.Collections.ObjectModel;
using System.Windows;
using System.Windows.Automation.Peers;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;

namespace ThinkVisionControl.Ui.Controls;

/// <summary>
/// Left navigation pane (WinUI NavigationView, Left mode): a menu button, the main <see cref="NavigationItem"/>s and
/// <see cref="FooterItems"/> pinned at the bottom. Main and footer items share one selection, reported by
/// <see cref="SelectedNavigationItem"/>. The footer list handles its own SelectionChanged; use
/// <see cref="SelectedNavigationItem"/> rather than <see cref="Selector.SelectionChanged"/> to follow both.
/// </summary>
[TemplatePart(Name = "PART_PaneToggle", Type = typeof(ButtonBase))]
[TemplatePart(Name = "PART_FooterList", Type = typeof(Selector))]
public class NavigationPane : ListBox
{
    Selector? _footer;
    ButtonBase? _toggle;
    bool _syncing;

    static NavigationPane()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(NavigationPane), new FrameworkPropertyMetadata(typeof(NavigationPane)));
    }

    public NavigationPane() => SetValue(FooterItemsProperty, new ObservableCollection<NavigationItem>());

    /// <summary>Icons only, 48 px wide; otherwise <see cref="OpenPaneLength"/> wide.</summary>
    public static readonly DependencyProperty IsCompactProperty =
        DependencyProperty.Register(nameof(IsCompact), typeof(bool), typeof(NavigationPane), new FrameworkPropertyMetadata(false));

    public static readonly DependencyProperty OpenPaneLengthProperty =
        DependencyProperty.Register(nameof(OpenPaneLength), typeof(double), typeof(NavigationPane), new FrameworkPropertyMetadata(280.0));

    /// <summary>NavigationItems pinned to the bottom of the pane (e.g. App settings).</summary>
    public static readonly DependencyProperty FooterItemsProperty =
        DependencyProperty.Register(nameof(FooterItems), typeof(IList), typeof(NavigationPane));

    /// <summary>The selected item, main or footer. Settable; binds two-way by default.</summary>
    public static readonly DependencyProperty SelectedNavigationItemProperty =
        DependencyProperty.Register(nameof(SelectedNavigationItem), typeof(NavigationItem), typeof(NavigationPane),
            new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.BindsTwoWayByDefault,
                (d, e) => ((NavigationPane)d).Select(e.NewValue as NavigationItem)));

    /// <summary>
    /// Raised when the menu button is clicked. If no handler marks it handled, the pane toggles <see cref="IsCompact"/>;
    /// a narrow window can handle it and open the pane as an overlay instead.
    /// </summary>
    public static readonly RoutedEvent PaneToggleRequestedEvent =
        EventManager.RegisterRoutedEvent(nameof(PaneToggleRequested), RoutingStrategy.Bubble, typeof(RoutedEventHandler), typeof(NavigationPane));

    public bool IsCompact { get => (bool)GetValue(IsCompactProperty); set => SetValue(IsCompactProperty, value); }
    public double OpenPaneLength { get => (double)GetValue(OpenPaneLengthProperty); set => SetValue(OpenPaneLengthProperty, value); }
    public IList? FooterItems { get => (IList?)GetValue(FooterItemsProperty); set => SetValue(FooterItemsProperty, value); }
    public NavigationItem? SelectedNavigationItem { get => (NavigationItem?)GetValue(SelectedNavigationItemProperty); set => SetValue(SelectedNavigationItemProperty, value); }

    public event RoutedEventHandler PaneToggleRequested { add => AddHandler(PaneToggleRequestedEvent, value); remove => RemoveHandler(PaneToggleRequestedEvent, value); }

    protected override AutomationPeer OnCreateAutomationPeer() => new NavigationPaneAutomationPeer(this);

    // A ListBox peer only reports its own items, so the menu button and the pinned footer list (template parts)
    // would be invisible to screen readers and UI Automation. Report them as children too.
    sealed class NavigationPaneAutomationPeer(NavigationPane owner) : ListBoxAutomationPeer(owner)
    {
        protected override List<AutomationPeer> GetChildrenCore()
        {
            var pane = (NavigationPane)Owner;
            var children = base.GetChildrenCore() ?? [];
            if (pane._toggle is { } toggle && UIElementAutomationPeer.CreatePeerForElement(toggle) is { } togglePeer)
                children.Insert(0, togglePeer);
            if (pane._footer is { } footer && UIElementAutomationPeer.CreatePeerForElement(footer) is { } footerPeer)
                children.Add(footerPeer);
            return children;
        }
    }

    protected override bool IsItemItsOwnContainerOverride(object item) => item is NavigationItem;
    protected override DependencyObject GetContainerForItemOverride() => new NavigationItem();

    public override void OnApplyTemplate()
    {
        if (_footer != null) _footer.SelectionChanged -= FooterSelectionChanged;
        if (_toggle != null) _toggle.Click -= ToggleClicked;
        base.OnApplyTemplate();
        _footer = GetTemplateChild("PART_FooterList") as Selector;
        _toggle = GetTemplateChild("PART_PaneToggle") as ButtonBase;
        if (_toggle != null) _toggle.Click += ToggleClicked;
        if (_footer == null) return;
        _footer.SelectionChanged += FooterSelectionChanged;
        if (SelectedNavigationItem is { } item && FooterItems?.Contains(item) == true) Select(item);
    }

    protected override void OnSelectionChanged(SelectionChangedEventArgs e)
    {
        base.OnSelectionChanged(e);
        if (_syncing) return;
        if (SelectedItem != null && _footer != null) Sync(() => _footer.SelectedItem = null);
        Publish();
    }

    void FooterSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        e.Handled = true; // keep the footer's event from surfacing as this pane's SelectionChanged
        if (_syncing) return;
        if (_footer?.SelectedItem != null) Sync(() => SelectedItem = null);
        Publish();
    }

    void Publish()
    {
        var item = SelectedItem is { } selected ? ItemContainerGenerator.ContainerFromItem(selected) as NavigationItem ?? selected as NavigationItem
            : _footer?.SelectedItem as NavigationItem;
        Sync(() => SetCurrentValue(SelectedNavigationItemProperty, item));
    }

    void Select(NavigationItem? item)
    {
        if (_syncing) return;
        Sync(() =>
        {
            var inFooter = item != null && FooterItems?.Contains(item) == true;
            if (_footer != null) _footer.SelectedItem = inFooter ? item : null;
            SelectedItem = item == null || inFooter ? null : ItemContainerGenerator.ItemFromContainer(item) is { } data && data != DependencyProperty.UnsetValue ? data : item;
        });
    }

    void Sync(Action action)
    {
        _syncing = true;
        try { action(); }
        finally { _syncing = false; }
    }

    void ToggleClicked(object sender, RoutedEventArgs e)
    {
        var request = new RoutedEventArgs(PaneToggleRequestedEvent, this);
        RaiseEvent(request);
        if (!request.Handled) SetCurrentValue(IsCompactProperty, !IsCompact);
    }
}
