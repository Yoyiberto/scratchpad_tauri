import os
import subprocess
from PIL import Image

# Read original index.html
with open('src/index.html', 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Dark theme version with sample notes
dark_html = content.replace('data-theme="light"', 'data-theme="dark"')
dark_html = dark_html.replace('></textarea>', '>Notas de reunión:\n- Revisar la transcripción del audio\n- Quitar las palabras repetidas\n- Exportar el dataset final a CSV</textarea>')

with open('src/temp_dark.html', 'w', encoding='utf-8') as f:
    f.write(dark_html)

edge_exe = r'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
temp_user = os.path.join(os.environ.get('TEMP', '.'), 'edge_scr_user')

# Dark screenshot
dark_png = os.path.abspath('screenshot_dark.png')
subprocess.run([
    edge_exe,
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu',
    f'--user-data-dir={temp_user}',
    f'--screenshot={dark_png}',
    '--virtual-time-budget=2000',
    '--window-size=600,750',
    'file:///' + os.path.abspath('src/temp_dark.html').replace('\\', '/')
], check=True)

if os.path.exists('src/temp_dark.html'):
    os.remove('src/temp_dark.html')

# 2. Correction modal screenshot
modal_html = content.replace('id="corr-modal" class="modal-backdrop hidden"', 'id="corr-modal" class="modal-backdrop"')
modal_html = modal_html.replace('<div id="corr-orig-word" class="word-badge"></div>', '<div id="corr-orig-word" class="word-badge">quitar</div>')
modal_html = modal_html.replace('id="corr-new-word" class="form-input"', 'id="corr-new-word" class="form-input" value="remover"')
modal_html = modal_html.replace('<div id="corr-context" class="context-preview"></div>', '<div id="corr-context" class="context-preview">Notas de reunion:\n- Revisar la transcripcion del audio\n- <mark>quitar</mark> las palabras repetidas</div>')

with open('src/temp_modal.html', 'w', encoding='utf-8') as f:
    f.write(modal_html)

modal_png = os.path.abspath('screenshot_modal.png')
subprocess.run([
    edge_exe,
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu',
    f'--user-data-dir={temp_user}',
    f'--screenshot={modal_png}',
    '--virtual-time-budget=2000',
    '--window-size=600,750',
    'file:///' + os.path.abspath('src/temp_modal.html').replace('\\', '/')
], check=True)

if os.path.exists('src/temp_modal.html'):
    os.remove('src/temp_modal.html')

# 3. History modal screenshot
hist_html = content.replace('id="history-modal" class="modal-backdrop hidden"', 'id="history-modal" class="modal-backdrop"')
hist_html = hist_html.replace('0 corrections saved', '2 corrections saved')
sample_rows = '''
<tr>
  <td><span class="word-del">quitar</span></td>
  <td><span class="word-ins">remover</span></td>
  <td class="context-cell">Notas de reunion: - Revisar la transcripcion del audio - <mark>quitar</mark> las palabras repetidas</td>
  <td class="date-cell">18/9/2026, 19:30</td>
</tr>
<tr>
  <td><span class="word-del">windwos</span></td>
  <td><span class="word-ins">Windows</span></td>
  <td class="context-cell">Tal vez tengas que adaptar un poco para <mark>windwos</mark></td>
  <td class="date-cell">18/9/2026, 19:32</td>
</tr>
'''
hist_html = hist_html.replace('<tbody id="corrections-tbody">\n              </tbody>', f'<tbody id="corrections-tbody">{sample_rows}</tbody>')

with open('src/temp_hist.html', 'w', encoding='utf-8') as f:
    f.write(hist_html)

hist_png = os.path.abspath('screenshot_history.png')
subprocess.run([
    edge_exe,
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu',
    f'--user-data-dir={temp_user}',
    f'--screenshot={hist_png}',
    '--virtual-time-budget=2000',
    '--window-size=600,750',
    'file:///' + os.path.abspath('src/temp_hist.html').replace('\\', '/')
], check=True)

if os.path.exists('src/temp_hist.html'):
    os.remove('src/temp_hist.html')

print('All screenshots generated successfully')
