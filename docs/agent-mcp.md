# MCP управления открытой композицией

Сервер включается переменной `VIDEO_KADR_MCP_TOKEN` (32–256 печатных ASCII-символов). Используйте случайный ключ. Endpoint: `http://127.0.0.1:8080/mcp`, bearer authentication. Для подключения откройте Multitrack → «Агент и история», введите тот же ключ и нажмите «Подключить агента». Backend проверяет loopback socket peer, Host, Origin и bearer; удалённое подключение запрещено даже при LAN bind.

Транспорт: JSON-RPC через HTTP POST, MCP protocol `2025-03-26`, JSON responses. GET/SSE пока не реализован. Каталог: `initialize`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`. Ресурсы `project://document` и `project://history` читают тот же live editor. При `initialize` сервер выдаёт `Mcp-Session-Id`; клиент передаёт его в следующих запросах. У каждой сессии отдельный actor; `DELETE /mcp` отзывает сессию и освобождает её блокировку через следующий editor heartbeat. Для stateless-клиентов можно передать уникальный `X-Agent-Id`; без обоих заголовков используется общий actor `mcp-client`. Сессии ограничены 128 и истекают через 30 минут без запросов.

Инструменты: `get_project`, `get_history`, `apply_batch`, `dry_run`, `create_checkpoint`, `restore_checkpoint`, `delete_checkpoint`, `set_history_lock`, `undo`, `redo`.

`get_project` возвращает текущий документ и `revision`. Передайте её как `expectedRevision` в `apply_batch` или `dry_run`. Если пользователь уже изменил проект, запрос отклоняется. Все времена — целые микросекунды, время композиции начинается с 0.

Пример `tools/call`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "apply_batch",
    "arguments": {
      "expectedRevision": 0,
      "operations": [
        { "op": "add_track", "track": { "id": "captions", "kind": "text", "name": "Captions", "locked": false, "hidden": false, "clips": [] } },
        { "op": "add_layer", "trackId": "captions", "clip": { "id": "title", "kind": "text", "timelineStartTicks": 0, "durationTicks": 2000000, "text": "Hello", "x": 0, "y": 0, "opacity": 1, "style": { "fontSizePx": 48, "color": "#FFFFFFFF", "align": "center" } } }
      ]
    }
  }
}
```

| Операция | Поля |
|---|---|
| add_track | track (полный CompositionTrack), index? |
| delete_track | trackId |
| reorder_track | trackId, index |
| add_layer | trackId, clip (полный CompositionClip) |
| update_layer | clipId, patch (shallow merge; id/kind запрещены, итог валидируется) |
| delete_layer | clipId |
| move_layer | clipId, trackId, startTicks |
| trim_layer | clipId, startTicks, endTicks |
| split_layer | clipId, atTicks, rightClipId |
| add_marker | marker: {id,tick,label,color?,origin?} |
| remove_marker | markerId |
| add_transition | trackId, transition (полный CompositionTransition) |
| remove_transition | trackId, transitionId |

`update_layer` позволяет изменять существующие transform/audio keyframes, эффекты, маски, текст и настройки ретайминга согласно текущему CompositionClip. Схема типов: `frontend/src/lib/composition/types.ts`; проверка: `validation.ts`. Batch ограничен 100 операциями и проверяется полностью на копии, потом фиксируется одной записью истории. Ошибка любой операции оставляет live document/history без изменений. `dry_run` ничего не фиксирует.

Контрольные точки включают документ, media mapping и настройки экспорта; хранятся в памяти открытого проекта, максимум 20. Восстановление — обычное undoable изменение. При смене проекта точки и блокировка удаляются. `set_history_lock {locked:true,reason}` защищает undo/redo/restore от других actors на 60 секунд; владелец может продлить блокировку повторным вызовом. Блокировка не запрещает обычные правки пользователя, поэтому revision всегда проверяется перед batch.

Команды обрабатывает один подключённый редактор. Второй получает 409, пока первый отправляет heartbeat. Команда выдаётся ровно один раз; timeout, disconnect или потеря acknowledgement не приводят к автоматическому повтору. Если сервер не получил подтверждение за 15 секунд (240 секунд для анализа сцен), агент обязан перечитать проект перед повтором. Закрытие панели отменяет polling и освобождает блокировку истории. Журнал Started/Ok/Err ограничен 200 событиями; последние 20 показаны в панели.

Это первый рабочий слой MCP, а не полный паритет с WeftCut: change feeds, библиотечные/render tools, расширенные compute tools, постоянные checkpoints и Motifs остаются в `weftcut-parity.md`.

## Анализ сцен

`analyze_clip {clipId, threshold?, minShotTicks?, disabledCuts?, stats?}` возвращает source-absolute floor scan, сцены на часах композиции и необязательные показатели. `measure_shots` принимает те же поля и измеряет яркость, движение, резкость и black/freeze/fade. `compare_frames {clipId,leftTick,rightTick}` сравнивает два абсолютных source ticks внутри окна клипа. Все расчёты выполняет локальный Rust/FFmpeg backend, редактор хранит только результат.

`auto_split_by_shot`, `mark_shots`, `apply_shot_cuts` требуют `clipId` и `expectedRevision`; принимают параметры обзора выше. `apply_shot_cuts` дополнительно принимает `discardedRowIds` из ответа `analyze_clip`. Изменение атомарно и отменяется одним Undo. Границы одинаково привязываются к сетке кадров. Нельзя исключить все сцены, изменить locked track или разделить freeze clip. Изменение проекта во время расчёта, timeout или disconnect запрещают поздний commit.

Один скан всего источника при floor 0.05 кешируется по полному SHA256 файла и версии FFmpeg; threshold/minimum не запускают повторный расчёт. Показатели кешируются отдельно по source hash и диапазону. Source access проверяется по тем же правам Space, что и библиотека.

Связи: batch create_link {id,memberIds,reassign?} и remove_link {id}. move_layer/trim_layer/split_layer распространяются по связи; escapeLink:true ограничивает действие названным клипом. Delete остаётся локальным; undersized links растворяются. Trim затрагивает только совпадающие края и ограничивает расширение общим запасом source range; duplicate_layer создаёт копии участников в отдельной связи; escapeLink:true копирует только названный клип. Поля: clipId, id, trackId?, startTicks?, linkId?, escapeLink?.

`paste_layers` в batch принимает `subjects:[{clipId,id}]`, `trackId?`, `startTicks?`, `linkIds?`. Копирует строго названный набор, не расширяя связи. Первый subject задаёт позицию/дорожку anchor; остальные сдвигаются на общую дельту на своих дорожках. Между копиями восстанавливаются только исходные link subsets с 2+ участниками. linkIds соответствует порядку таких subsets в document.links. Вся операция атомарна.

`move_layers` в batch: `{clipIds:[...],trackId,startTicks}`. Перемещает точный набор общей дельтой, первый id — anchor и единственный участник, меняющий дорожку. Link membership не расширяет список. Shared origin clamp сохраняет взаимные смещения. Любой locked/colliding member отменяет весь batch.

`move_layer_placements`: `{placements:[{clipId,trackId},...],anchorId,startTicks}`. Каждый subject получает собственную дорожку назначения; anchorId обязан входить в уникальный набор. Requested head сначала округляется на сетку anchor, общая дельта ограничивается началом timeline, затем каждый endpoint округляется на own frame/sample grid. Связи не расширяют subjects. Все назначения проходят kind/lock/transition/collision admission; отказ любого участника отменяет весь batch. Это API для явных назначений, а не создание универсальной смешанной дорожки.

`ripple_delete_layers` в batch: `{clipIds:[...]}` удаляет точный набор и закрывает объединение освобождённых интервалов на всех дорожках. `ripple_delete_gap`: `{trackId,startTicks,endTicks}` закрывает один существующий пробел с точными границами, не удаляя клипы. Частичный или устаревший диапазон отклоняется. Обе операции атомарно проверяют блокировки, содержимое внутри отверстия, связи через границу и конечные пересечения.

`set_layers_enabled`: `{clipIds:[...],enabled:boolean}` меняет точный набор без link fan-out. Отключённые клипы исключаются из браузерного предпросмотра. Экспорт передаёт флаг в серверный контракт; отключённые клипы исключаются из активного набора. Пробелы основной видеодорожки заполняются фоном и тишиной, включая дорожки с переходами между соседними активными клипами.

`set_layer_locked {clipId,locked}` меняет только блокировку клипа, в том числе на locked track. Изменения защищённого содержимого, источника, связей и переходов отклоняются после каждой batch-операции. Для unlock + edit нужны отдельные операции; batch остаётся атомарным. Исторические revert-пути сохраняют отдельную проверку history lock.
