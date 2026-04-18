
# 🎵 Playlist-Player

Un reproductor de música ligero y enfocado en el almacenamiento local, construido con **Tauri** y **Rust**. Esta aplicación convierte cualquier lista de reproducción de YouTube en una biblioteca local, manteniendo tu música actualizada en tu equipo.

https://github.com/user-attachments/assets/4f7f9500-e655-44fb-a3e7-892e6786dbaa

## ✨ Características

* **Sincronización con YouTube**: Solo proporciona la URL de una lista de reproducción y la aplicación utiliza `yt-dlp` para descargar y sincronizar la música localmente.
* **Actualización Automática**: Mantiene tu biblioteca de música local siempre al día con los cambios en la fuente de YouTube.
* **Reproducción Aleatoria (Shuffle-Only)**: Diseñado para una experiencia sin complicaciones: la música se reproduce exclusivamente en orden aleatorio.
* **Integración con la Bandeja del Sistema (Tray)**: <img src="https://github.com/user-attachments/assets/89b9b2c3-ac55-4754-b50f-752f70d50d7e" align="right" width="178" height="214" alt="Tray Menu"> Control total desde el icono de la barra de tareas. Puedes gestionar la reproducción, saltar pistas o cambiar la visibilidad de la ventana sin necesidad de abrir la interfaz principal.

* **Gestión Inteligente de Ventanas**: 
    * Botones internos ocultos para ocultar o cerrar la ventana instantáneamente.
* **Atajos de Teclado**:
    * **Locales**: Controles estándar cuando la aplicación está enfocada.
    * **Globales**: Controla tu música incluso mientras trabajas en otras aplicaciones.
