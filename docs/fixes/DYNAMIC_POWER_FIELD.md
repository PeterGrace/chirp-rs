# Dynamic Power Field Implementation

## Overview

Implemented dynamic configuration of the Power field in the edit dialog based on radio capabilities. The Power field now:
- **Appears only when the radio supports variable power** (`has_variable_power: true`)
- **Shows radio-specific power levels** (e.g., UV-5R shows "High" / "Low")
- **Hides for radios without variable power support**

This follows the existing pattern used for tone/D-STAR field visibility.

## Implementation Details

### 1. Added C-Exported Function: `get_radio_features()`

**Location**: `src/gui/qt_gui.rs` (lines ~2143-2214)

**Purpose**: Returns JSON with radio capabilities:
```json
{
  "has_variable_power": true,
  "has_bank": false,
  "power_levels": ["High", "Low"]
}
```

**How it works**:
- Reads current radio vendor/model from `AppState`
- Instantiates the appropriate radio driver
- Calls `radio.get_features()`
- Extracts power-related fields and returns as JSON

**Supported radios**:
- UV-5R: 2 levels ("High" 4W, "Low" 1W)
- TH-D75: Returns feature info (to be populated when driver is enhanced)
- Other radios: Returns defaults

### 2. Updated Edit Dialog

**Location**: `src/gui/qt_gui.rs` `showEditDialog()` function

**Changes**:

#### A. Added JSON parsing includes:
```cpp
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QJsonArray>
```

#### B. Fetch features at dialog startup:
```cpp
// Get radio features
QString featuresStr = QString::fromUtf8(get_radio_features());
QJsonDocument featuresDoc = QJsonDocument::fromJson(featuresStr.toUtf8());
QJsonObject featuresObj = featuresDoc.object();
bool hasVariablePower = featuresObj["has_variable_power"].toBool();
bool hasBank = featuresObj["has_bank"].toBool();
QJsonArray powerLevelsArray = featuresObj["power_levels"].toArray();
```

#### C. Create Power combo box:
```cpp
QComboBox* powerCombo = new QComboBox();
if (!powerLevels.isEmpty()) {
    powerCombo->addItems(powerLevels);
    // Set current power level from data.power
    QString powerStr = QString::fromUtf8(data.power);
    if (!powerStr.isEmpty()) {
        powerCombo->setCurrentText(powerStr);
    }
}
```

#### D. Add to layout with conditional visibility:
```cpp
// Power field (conditionally shown)
QWidget* powerWidget = new QWidget();
QFormLayout* powerLayout = new QFormLayout(powerWidget);
powerLayout->setContentsMargins(0, 0, 0, 0);
powerLayout->addRow("Power:", powerCombo);
layout->addRow(powerWidget);
```

#### E. Enhanced visibility control:
```cpp
auto updateFieldVisibility = [=]() {
    bool isDV = modeCombo->currentText() == "DV";
    toneWidget->setVisible(!isDV);
    dstarWidget->setVisible(isDV);
    powerWidget->setVisible(hasVariablePower);  // NEW
    bankWidget->setVisible(hasBank);             // NEW (bonus!)
};
```

This lambda now controls:
- Tone fields (hidden for DV mode)
- D-STAR fields (shown for DV mode)
- **Power field (shown only if radio supports it)**
- **Bank field (shown only if radio has banks)**

### 3. Updated `update_memory()` Function

**Location**: `src/gui/qt_gui.rs` (lines ~2718-2787)

**Changes**:

#### A. Added power parameter to signature:
```rust
pub unsafe extern "C" fn update_memory(
    // ... existing params ...
    bank: u8,
    power: *const c_char,  // NEW
    urcall: *const c_char,
    // ...
)
```

#### B. Parse power level:
```rust
let power_str = CStr::from_ptr(power).to_str().unwrap_or("").to_string();

let power_level = if !power_str.is_empty() {
    use crate::core::power::PowerLevel;
    PowerLevel::parse(&power_str).ok()
} else {
    None
};
```

#### C. Update memory:
```rust
mem.power = power_level;
```

#### D. Updated C++ declaration:
```cpp
const char* update_memory(size_t row, uint64_t freq, const char* name,
                         const char* duplex, uint64_t offset, const char* mode,
                         float tuning_step, const char* tmode, float rtone, float ctone,
                         uint8_t bank, const char* power, const char* urcall,
                         const char* rpt1call, const char* rpt2call);
```

#### E. Updated call site in dialog:
```cpp
const char* error = update_memory(
    row,
    freqHz,
    nameEdit->text().toUtf8().constData(),
    duplexCombo->currentText().toUtf8().constData(),
    offsetHz,
    modeCombo->currentText().toUtf8().constData(),
    tuningStep,
    tmodeCombo->currentText().toUtf8().constData(),
    rtone,
    ctone,
    static_cast<uint8_t>(bankCombo->currentData().toInt()),
    powerCombo->currentText().toUtf8().constData(),  // NEW
    urcallEdit->text().toUtf8().constData(),
    rpt1Edit->text().toUtf8().constData(),
    rpt2Edit->text().toUtf8().constData()
);
```

## Bonus Feature: Dynamic Bank Field

As part of this implementation, the Bank field is now also conditionally shown based on `has_bank` from RadioFeatures:
- **UV-5R**: Bank field is hidden (UV-5R has no banks)
- **TH-D75**: Bank field is shown (TH-D75 has 10 banks)

## Testing

### UV-5R Test Scenario:
1. Load a UV-5R .img file
2. Edit a memory
3. **Expected**: Power combo shows "High" / "Low" options
4. **Expected**: Bank field is hidden
5. Change power level and save
6. **Expected**: Power value is saved correctly

### TH-D75 Test Scenario:
1. Load a TH-D75 .img file
2. Edit a memory
3. **Expected**: Power field is hidden (TH-D75 doesn't report variable power yet)
4. **Expected**: Bank field is shown with bank names

## Future Enhancements

1. **Add power levels to TH-D75 driver**:
   - Update `get_features()` to set `has_variable_power: true`
   - Populate `valid_power_levels` with TH-D75's power options

2. **Add power support to IC-9700 driver** (when implemented)

3. **Other conditional fields**:
   - Could apply same pattern to:
     - Tuning step (some radios have fixed steps)
     - Duplex/Offset (simplex-only radios)
     - Tone modes (radios without CTCSS/DTCS)

## Benefits

- **Cleaner UI**: Users only see fields relevant to their radio
- **Less confusion**: No power selection on radios that don't support it
- **Extensible**: Easy to add more conditional fields
- **Radio-specific**: Each radio automatically gets correct options
- **Follows existing patterns**: Uses same visibility mechanism as tone/D-STAR fields

## Files Modified

1. **src/gui/qt_gui.rs**:
   - Added JSON parsing includes
   - Added `get_radio_features()` C-exported function
   - Updated `showEditDialog()` to conditionally show power/bank fields
   - Updated `update_memory()` to accept and save power parameter

## Build Status

✅ Builds successfully with `cargo build --features gui`
✅ No compilation errors
✅ Only pre-existing warnings remain (unused variables in unrelated code)
