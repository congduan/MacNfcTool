# Mac NFC Tool

A simple and intuitive NFC tool for Mac, designed for reading and writing Mifare Classic cards using PN532 module.

## Features

- **Device Management**: Connect and disconnect NFC readers
- **Card Information**: Read basic card information (UID, ATQA, SAK, card type)
- **Sector Operations**: Read and write individual sectors
- **Batch Operations**: Read all 16 sectors at once
- **Data Files**: Save and load card data to/from JSON files
- **Key Management**: Preset common keys for easy access
- **Quick Filtering**: Fast sector navigation with numbered buttons
- **Responsive Design**: Adapts to different window sizes

## Requirements

- MacOS
- Node.js 18+
- PN532 NFC module
- Tauri development environment

## Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/MacNfcTool.git
   cd MacNfcTool
   ```

2. **Install dependencies**
   ```bash
   npm install
   ```

3. **Build the project**
   ```bash
   npm run build
   ```

4. **Run the application**
   ```bash
   npm run dev
   ```

## Usage

### Connecting to a Reader
1. Connect your PN532 NFC module to your Mac
2. Click "连接读卡器" to establish a connection
3. The device information will be displayed in the sidebar

### Reading Card Information
1. Place a Mifare Classic card on the reader
2. Click "读取卡片信息" to get basic card details

### Reading Sectors
1. Select a sector using the quick filter buttons (0-15)
2. Click the "读取" button on the sector card
3. The sector data will be displayed in hex format

### Writing Sectors
1. Select a sector using the quick filter buttons (0-15)
2. Click the "写入" button on the sector card
3. Enter the data you want to write

### Reading All Sectors
1. Click the "读取全部数据" button in the top right corner
2. All 16 sectors will be read and displayed

### Saving/Loading Data
1. Enter a file path in the "文件路径(JSON)" field
2. Click "读取并保存16扇区" to save all sector data to a file
3. Click "从文件加载预览" to load data from a file
4. Click "将文件写入卡片" to write data from a file to the card

## Key Presets

The tool includes common Mifare Classic keys:
- `FFFFFFFFFFFF` (Default factory key)
- `A0A1A2A3A4A5` (NXP default key)
- `D3F7D3F7D3F7` (Common key)
- `000000000000` (All zeros key)

You can also select between Key A and Key B for authentication.

## Troubleshooting

### Common Errors
- **Mifare Authentication Failed**: Check if the correct key and key type are selected
- **No card detected**: Ensure the card is properly placed on the reader
- **Reader not found**: Check the USB connection and driver installation

### Tips
- Always keep a backup of your card data before writing
- Be cautious when writing to sector B3 (trailer block), as this contains key information
- Use the log panel at the bottom right to monitor operations

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT License
