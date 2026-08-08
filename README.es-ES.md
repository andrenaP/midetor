

# midetor

## Descripción

`midetor` (MY-EDITOR) es un editor de Markdown basado en la terminal y similar a Vim, diseñado para ofrecer una experiencia altamente personalizable y al estilo de Obsidian directamente en tu terminal. Se integra estrechamente con [markdown-scanner](https://github.com/andrenaP/markdown-scanner) para proporcionar una gestión de metadatos rápida, respaldada por SQLite. 

Impulsado por Ratatui y Crossterm, `midetor` ahora cuenta con **scripting de Lua integrado**, lo que te permite personalizar completamente los atajos de teclado, escribir proveedores de búsqueda personalizados y crear tus propios fragmentos de autocompletado. 

## Probándolo
Ve a [este repositorio](https://github.com/andrenaP/midetor-docker-tesiting) y ejecútalo dentro de `Docker`. Puedes pasar `-v` para montar tu carpeta como un volumen si lo deseas.

## ¿Por qué?
- ¿Buscas una experiencia similar a `nvim` que no se desconfigure varias veces por semana? Este editor es altamente autónomo. 
- **Simplemente puedes copiarlo en cualquier dispositivo con una terminal y funcionará directamente sin configuración adicional.**
- Puedes usar [este sitio web](https://github.com/andrenaP/database-reader-sql) para renderizar tus datos en una interfaz amigable para el usuario.
- Es completamente **extensible mediante Lua** sin necesidad de recompilar el código fuente en Rust.
- Este editor sirve como un ejemplo avanzado de cómo puedes trabajar con `markdown-scanner`.

![images/main.jpg](https://github.com/andrenaP/midetor/blob/aadcee84d86bc2e4686d600950c919c017e5a820/images/main.jpg)

¡Ahora puede renderizar imágenes (usando backlinks) directamente en la terminal!
![images/images-render-example.png](https://github.com/andrenaP/midetor/blob/2c23333e6a1ea811a73961963ba739051a3099f1/images/images-render-example.png)

## Características

- **Configuración extensible:** Escribe un archivo `init.lua` para definir mapas de teclas, macros y comportamientos de la interfaz.
- **Reproducción de medios:** Renderiza imágenes locales en la terminal y reproduce archivos de audio `.mp3` vinculados a través de VLC.
- **Edición avanzada:** Modos similares a Vim: Normal, Inserción, Visual y Comando.
- **Texto virtual e interfaz:** Inyecta superposiciones de texto personalizadas en el búfer del editor mediante Lua.
- **Soporte para grafo de conocimiento:** Gestiona etiquetas (`#`) y backlinks (`[[`) a través de una base de datos SQLite rápida.
- **Búsqueda y autocompletado personalizados:** Programa tus propios buscadores difusos y expandidores de fragmentos usando Lua (activador `@`).

## Requisitos

- **Rust**: Versión 1.87.0 o superior.
- **Cargo**: El gestor de paquetes de Rust.
- **markdown-scanner**: Debe estar disponible en tu PATH del sistema para poblar la base de datos.
- **VLC (Opcional)**: Necesario si deseas reproducir enlaces de audio desde el editor.

## Instalación

1. **Instalar midetor**:
   Copia el binario compilado a un directorio en tu PATH:
   ```bash
   cargo install --git https://github.com/andrenaP/midetor.git
   ```

2. **Instalar `markdown-scanner`**:
   El editor requiere este binario para procesar archivos Markdown y generar la base de datos.
   ```bash
   cargo install --git https://github.com/andrenaP/markdown-scanner.git
   ```

## Uso

Ejecuta el editor con el siguiente comando:

```bash
midetor <file_path> [base_dir] [music_folder]
```

- `<file_path>`: Ruta al archivo Markdown a editar (obligatorio).
- `[base_dir]`: Directorio base del vault de Obsidian (opcional). Por defecto, usa la variable de entorno `Obsidian_valt_main_path` o el directorio de trabajo actual.
- `[music_folder]`: Directorio donde se almacenan los archivos `.mp3` (opcional). Por defecto, usa la variable de entorno `musik_folder` o el directorio de trabajo actual.

### Ejemplos

- Editar un archivo usando la ruta predeterminada del vault:
  ```bash
  midetor notes.md
  ```

- Editar un archivo con un directorio de vault específico y una carpeta de música:
  ```bash
  midetor notes.md /path/to/vault /path/to/music
  ```

## Configuración y scripting en Lua

`midetor` lee un archivo `init.lua` desde tu directorio de trabajo actual al iniciar o desde `~/,config/midetor/`. Aquí es donde configuras todos los atajos de teclado, macros personalizadas, lógica de búsqueda y expansiones de fragmentos.

El editor expone un objeto global `editor` a Lua.

### Mapeo de teclas
Puedes mapear teclas para los modos Normal (`n`) y Visual (`v`). Usa caracteres estándar o corchetes angulares para teclas especiales (p. ej., `<C-s>`, `<Esc>`, `<A-Up>`).

```lua
-- Guardar archivo
editor:map("n", "<C-s>", function() editor:save() end)

-- Alternar árbol de archivos
editor:map("n", "\\t", function() editor:toggle_file_tree() end)

-- Crear una macro personalizada (Ir al final de la línea, añadir salto de línea, entrar al modo de inserción)
editor:map("n", "o", function()
    editor:move("end")        
    editor:insert_text("\n")  
    editor:set_mode("insert") 
end)
```

### Fragmentos de autocompletado personalizados (`@`)
Puedes definir fragmentos dinámicos en Lua que se activen cuando escribas `@` en el modo de inserción. Debes definir dos funciones globales: `on_autocomplete` y `expand_autocomplete`.

```lua
local snippets = {
    ["date"] = function() return os.date("%Y-%m-%d") end,
    ["file-name"] = function() return editor:get_current_file():match("^.+/(.+)$") end
}

function on_autocomplete(trigger, query)
    local results = {}
    if trigger == "@" then
        for key, _ in pairs(snippets) do
            if string.sub(key, 1, string.len(query)) == query then
                table.insert(results, key)
            end
        end
    end
    return results
end

function expand_autocomplete(trigger, suggestion)
    if trigger == "@" then
        local action = snippets[suggestion]
        if type(action) == "function" then return action() end
        if type(action) == "string" then return action end
    end
    return suggestion 
end
```

### Proveedores de búsqueda personalizados
Puedes crear interfaces de búsqueda personalizadas directamente en Lua proporcionando una función de búsqueda y una función de devolución de llamada (callback) para la selección.

```lua
_G.my_search_provider = function(query)
    -- Devuelve una tabla de cadenas basada en la consulta
    return {"apple", "banana", "cherry"}
end

_G.my_search_action = function(selected_item)
    editor:insert_text("Selected: " .. selected_item)
end

-- Vincularlo a una tecla
editor:map("n", "\\fs", function()
    editor:start_custom_search("my_search_provider", "my_search_action")
end)
```

## Atajos de teclado predeterminados (proporcionados mediante `init.lua`)

Si usas el `init.lua` de ejemplo, los siguientes atajos predeterminados estarán activos:

| Atajo | Modo | Acción |
| :--- | :--- | :--- |
| `i` / `a` | Normal | Entrar al modo de inserción |
| `v` / `<C-v>` | Normal | Entrar al modo Visual / Bloque Visual |
| `:` | Normal | Entrar al modo de comando (`:w`, `:q`, `:wq`) |
| `<C-s>` / `<C-q>`| Normal | Guardar archivo / Salir |
| `u` / `<C-r>` | Normal | Deshacer / Rehacer |
| `\t` | Normal | Alternar árbol de archivos |
| `\ob` / `\ot` | Normal | Buscar backlinks / Buscar etiquetas |
| `\f` | Normal | Buscar archivos |
| `\os` | Normal | Buscar mediante consulta SQL personalizada |
| `\if` / `\ic` | Normal | Alternar imagen a pantalla completa / Limpiar imagen |
| `\s` | Normal | Detener reproducción de audio |
| `\w` | Normal | Alternar modo de lectura (ajuste de líneas) |
| `\nt` | Normal | Abrir selector de plantillas |
| `\oot` | Normal | Abrir la nota diaria de hoy |

*Nota: El árbol de archivos tiene sus propios atajos una vez abierto (`oc` para ordenar por tiempo, `on` para ordenar por nombre, `y` para copiar, `x` para cortar, `p` para pegar, `d` para eliminar).*

## Base de datos

El editor crea una base de datos SQLite (`markdown_data.db`) en el `base_dir` para almacenar metadatos. El esquema incluye:
- `files`: Rutas y nombres de archivos.
- `tags`: Etiquetas únicas.
- `file_tags`: Mapeo de archivos a etiquetas.
- `backlinks`: Rastrea `[[backlinks]]` entre archivos.

La herramienta `markdown-scanner` se ejecuta automáticamente en segundo plano para mantener esta base de datos actualizada.

## Variables de entorno

- `OBSIDIAN_VAULT_PATH`: Directorio base predeterminado para el vault.
- `MUSIC_FOLDER`: Directorio predeterminado para la reproducción de audio/música.

## Licencia

Este proyecto está licenciado bajo la LICENCIA PÚBLICA GENERAL DE GNU. Consulta el archivo `LICENSE` para más detalles.

## Contacto

Para informes de errores o preguntas, por favor abre un issue en el repositorio.
