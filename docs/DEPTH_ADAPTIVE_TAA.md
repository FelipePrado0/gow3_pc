# Depth-Adaptive TAA Improvements

**Дата:** 2026-10-01  
**Статус:** Исторический эксперимент, заменён исправлением temporal reprojection.

Описанные ниже пороги и веса больше не используются. Уменьшение веса истории вдали
усиливало мерцание; сравнение device depth между разными камерами и хранение глубины
в FP16 не позволяли корректно отличать дальние поверхности.

## Проблема

Текущая реализация TAA использует фиксированные пороги для:
1. Определения disocclusion (раскрытие новых поверхностей): `0.0005`
2. Временного веса накопления: `0.9` → `0.65` в зависимости от движения

Эти параметры не учитывают глубину пикселя, что приводит к проблемам:
- **Далёкие объекты (depth → 1.0)**: временная нестабильность, мерцание
- **Близкие объекты (depth → 0.0)**: чрезмерное размытие при быстром движении

## Решение: Адаптивные пороги по глубине

### 1. Depth-Adaptive Disocclusion Threshold

**Файл:** `gpu/shadps4/video_core/host_shaders/taa.comp`

```glsl
float depthThreshold(float depth) {
    // Near: 0.001, Far (>0.95): 0.0002
    return mix(0.001, 0.0002, smoothstep(0.7, 0.95, depth));
}
```

**Логика:**
- Близкие поверхности (depth < 0.7): порог `0.001` — допускает небольшие расхождения
- Дальние поверхности (depth > 0.95): порог `0.0002` — строже, меньше false positives
- Плавный переход через `smoothstep(0.7, 0.95, depth)`

**Применение:**
```glsl
float depthDiff = abs(history.a - depth);
float threshold = depthThreshold(depth);
if (depthDiff < threshold && ...) {
    // Accept history
}
```

### 2. Depth-Adaptive Temporal Weight

```glsl
float temporalWeight(float depth, float motionLength) {
    // Base weight decreases for distant objects: 0.9 near, 0.8 far
    float baseWeight = mix(0.9, 0.8, smoothstep(0.7, 0.95, depth));
    // Motion reduces weight more aggressively for distant pixels
    float motionFactor = mix(0.65, 0.5, smoothstep(0.7, 0.95, depth));
    return mix(baseWeight, motionFactor, clamp(motionLength / 16.0, 0.0, 1.0));
}
```

**Логика:**
- **Базовый вес без движения:**
  - Близко: `0.9` (агрессивное накопление истории)
  - Далеко: `0.8` (консервативнее, меньше мерцания)
  
- **При быстром движении (motionLength > 16px):**
  - Близко: снижается до `0.65`
  - Далеко: снижается до `0.5` (ещё консервативнее)

**Применение:**
```glsl
float motionLength = length(motion);
float weight = temporalWeight(depth, motionLength);
result = mix(current, clipped, weight);
```

### 3. Motion Vector Validation (camera_motion.comp)

**Файл:** `gpu/shadps4/video_core/host_shaders/camera_motion.comp`

```glsl
// Depth-adaptive validation: distant surfaces need tighter depth matching.
float depth_threshold = mix(0.001, 0.0003, smoothstep(0.7, 0.95, depth));
if (object_motion.b > 0.99 && abs(object_motion.a - depth) < depth_threshold &&
    !any(isnan(object_motion.rg)) && !any(isinf(object_motion.rg))) {
    result = object_motion.rg;
}
```

**Что изменено:**
- Старый порог: фиксированный `0.001`
- Новый порог: `0.001` → `0.0003` для далёких поверхностей
- Предотвращает использование object motion vectors от неправильных пикселей

### 4. Улучшенная визуализация debug режимов

**Файл:** `gpu/shadps4/video_core/host_shaders/camera_motion.comp`

Режимы `GOW3_DEBUG_MOTION=1` теперь структурированы:
```glsl
if (mode == 1) {
    // Raw depth visualization
    imageStore(color_img, pixel, vec4(fract(depth * 4.0), depth, depth < 0.0 ? 1.0 : 0.0, 1.0));
} else if (mode == 2) {
    // View-space depth visualization (z / 50)
    const float v = z / 50.0;
    imageStore(color_img, pixel, vec4(fract(v * 4.0), v, v < 0.0 ? 1.0 : 0.0, 1.0));
}
```

## Ожидаемые улучшения

### Дальние объекты
- ✅ Меньше мерцания на далёких стенах и небе
- ✅ Более стабильная история при небольших движениях камеры
- ✅ Меньше ложных disocclusion от численных ошибок глубины

### Близкие объекты
- ✅ Меньше размытия при быстрых движениях (персонаж, оружие)
- ✅ Более точные object motion vectors
- ✅ Лучший баланс между накоплением и отзывчивостью

### Адаптивная зона (depth 0.7-0.95)
- Плавный переход между режимами
- Предотвращает резкие артефакты на границе

## Тестирование

### Визуальная проверка
```bash
# TAA в нативном разрешении
GOW3_UPSCALER=taa bash run.sh

# Проверить:
# 1. Мерцание далёких стен (тестовая сцена, верхние этажи)
# 2. Размытие оружия при быстром повороте
# 3. Стабильность при медленном движении камеры
# 4. Disocclusion при обходе углов
```

### Debug режимы
```bash
# Визуализация глубины
GOW3_DEBUG_MOTION=1 bash run.sh
# Toggle: (1<<21) = raw depth, (1<<22) = view-space z

# Проверка motion vectors
# Toggle: (1<<20) = blended motion, (1<<23) = reprojection, (1<<24) = difference
```

### A/B сравнение
```bash
# Создать файл для runtime-переключения
echo "0" > /tmp/gow3_toggle

# В игре переключать биты для сравнения
# Потребуется временный механизм для старых/новых весов
```

## Потенциальные риски

1. **Слишком консервативные веса на далёких объектах**
   - Может увеличить смазывание при быстрых поворотах
   - Решение: настроить `baseWeight` (0.8 → 0.85?)

2. **Слишком строгий порог глубины для object motion**
   - Может отбрасывать валидные векторы
   - Решение: `0.0003` → `0.0005` для depth > 0.95

3. **Smoothstep диапазон 0.7-0.95 может быть неоптимальным**
   - Игра, на которой калибровали: near=0.05, far=3000
   - Большинство геометрии в диапазоне 0.1-0.9
   - Решение: профилирование распределения глубины в типичных сценах

## Следующие шаги

1. ✅ Реализовать depth-adaptive пороги
2. ⏳ Собрать и протестировать в игре
3. ⏳ Собрать статистику распределения глубины
4. ⏳ Настроить пороги на основе реальных данных
5. ⏳ Добавить runtime-переключение для A/B тестов
6. ⏳ Документировать финальные параметры

## Ссылки

- [upscaler.md](upscaler.md) — общая архитектура апскейлера
- [motion_vectors.md](motion_vectors.md) — векторы движения
- TAA shader: `gpu/shadps4/video_core/host_shaders/taa.comp`
- Camera motion: `gpu/shadps4/video_core/host_shaders/camera_motion.comp`
