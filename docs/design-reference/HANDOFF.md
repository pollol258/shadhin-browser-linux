# Shadhin Browser — GTK4/libadwaita handoff

## Semantic colors (light / dark)
| Token | Light | Dark |
|---|---|---|
| window_bg | #fafafa | #242424 |
| view_bg | #ffffff | #1e1e1e |
| headerbar_bg | #f3f3f3 | #303030 |
| card_bg | #ffffff | #303030 |
| border_color | #d4d4d4 | #484848 |
| text_color | #202020 | #f5f5f5 |
| muted_text | #606060 | #b5b5b5 |
| accent_bg | #3584e4 | #3584e4 |
| accent_fg | #ffffff | #ffffff |
| success | #18794e | #57e389 |
| warning | #8c6000 | #f6d32d |
| error | #c01c28 | #ff7b63 |
| hover_bg | #e7e7e7 | #414141 |

The accent is a CSS variable set by the UI. In native GTK map these to AdwStyleManager and named CSS colors; adapt accent foreground for custom accent choices where needed.

## Foundation
Spacing: space_1=4px, space_2=8px, space_3=12px, space_4=16px, space_5=24px, space_6=32px. Radius: radius_sm=8px, radius_md=12px, radius_pill=999px. Typography: font_xs=12px, font_sm=13px, font_base=14px, font_lg=18px, font_title=26px. Family: Inter / Cantarell / Noto Sans Bengali. Shadows: shadow_1=0 1px 3px #00000012 (dark #0004); shadow_2=0 4px 16px #00000020 (dark #0008). Motion: 180ms standard, 120ms loading; disabled by prefers-reduced-motion.

## Component inventory and dimensions
| Component | Geometry | States / variants |
|---|---|---|
| Button | min-height 34px, padding 7px 13px, radius 8px | neutral, primary; hover, focus, disabled |
| IconButton | 36×36px (compact 32×32), icon 17px, radius 8px | default, hover, active, focus, disabled |
| Tab | 38px tall, padding 0 9px, radius 8px | active, hover, pinned, sleeping, audio, loading, close |
| AddressBar | 39px tall, pill radius, padding 0 3px | idle, focus, suggestion-open, secure/insecure |
| Popover | padding 12px, radius 12px, shadow_2 | open, closed, keyboard selection |
| ListRow | padding 12px, icon 19px | idle, hover, selected, action |
| ProgressBar | 5px tall, radius 8px | indeterminate concept, active, complete, error |
| Switch | 40×24px, 18px knob | on, off, focus, disabled |
| Dialog | width up to 460px, padding 25px, radius 12px | warning, confirmation |
| Toast | padding 12px 22px, radius 8px | visible, hidden |
| Banner | padding 10px 14px, radius 8px, 4px leading border | warning, error, low-memory |
| Card | padding 16–20px, radius 12px, shadow_1 | default, hover when interactive |

Minimum targets 32px. Use `:focus-visible` with a 2px accent outline. Interactive states use solid colors, not blur or gradients. Respect system reduced motion.

## Screen inventory
New Tab, Website Placeholder, Address Suggestions, Downloads (active/finished/failed/cancelled/missing/empty), Privacy & Permissions, Permission Prompt, Settings (six sections), History, Bookmarks, Context Menus, Offline Error, Certificate Warning, Low-Memory Banner, Sandbox Warning. Top tabs and vertical sidebar; 1280×800 and 1920×1080 fluid layouts.

## GTK4 mapping
React Button/IconButton → GtkButton; Tab → AdwTabView + AdwTabBar or GtkListBox; AddressBar → GtkEntry in GtkBox; Popover → GtkPopover; ListRow → AdwActionRow; ProgressBar → GtkProgressBar; Switch → GtkSwitch; Dialog → AdwAlertDialog; Toast → AdwToastOverlay; Banner → AdwBanner; Card → GtkFrame / AdwPreferencesGroup. Use `AdwStyleManager` for dark/light; CSS custom accent for theme accents. Backend must implement browser engine navigation, real download persistence, content blocking, permission mediation, secure connection state and sandbox detection. Prototype indicators are simulated and not security guarantees.

## Build
`npm install && npm run dev` then open the printed localhost URL. `npm run build` generates production static assets. No browser engine is included.
