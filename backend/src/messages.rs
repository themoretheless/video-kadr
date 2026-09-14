//! User-facing Russian error strings. Keep HTTP/job wording in one place so
//! i18n can later swap catalogs without hunting call sites.

pub const INVALID_URL: &str = "Недопустимый URL";
pub const QUEUE_CLOSED: &str = "очередь задач закрыта";
pub const API_ROUTE_NOT_FOUND: &str = "API-маршрут не найден";
pub const METHOD_NOT_ALLOWED: &str = "Метод не поддерживается";
pub const TOO_MANY_JOBS: &str = "Слишком много новых задач; повторите позже";
pub const BAD_COLOR_PARAMS: &str = "некорректные параметры цвета, chroma key или audio DSP";
pub const JOB_NOT_FOUND: &str = "Задача не найдена";
pub const JOB_ALREADY_FINISHED: &str = "Задача уже завершена";
pub const JOB_PAYLOAD_INCOMPATIBLE: &str = "Сохранённая задача имеет несовместимый формат";
pub const FAILED_JOB_NOT_FOUND: &str = "Неудачная задача не найдена";
pub const LUT_NOT_FOUND: &str = "LUT не найден";
pub const LUT_BAD_LINK: &str = "Некорректная ссылка на LUT";
pub const LUT_BAD_FILE_LINK: &str = "Некорректная ссылка на файл LUT";
pub const LUT_STORE_UNAVAILABLE: &str = "Хранилище LUT недоступно";
pub const LUT_FILE_MISSING: &str = "Файл LUT не найден";
pub const LUT_FILE_BAD_TYPE: &str = "Файл LUT имеет недопустимый тип";
pub const LUT_FILE_CORRUPT: &str = "Файл LUT повреждён";
pub const LUT_FILE_OUTSIDE_STORE: &str = "Файл LUT находится вне хранилища";
pub const LUT_FILE_VERIFY_FAILED: &str = "Не удалось проверить файл LUT";

pub const FILTER_CURVES_UNAVAILABLE: &str = "кривые недоступны: FFmpeg filter curves не найден";
pub const FILTER_HSL_UNAVAILABLE: &str = "HSL недоступен: FFmpeg filter huesaturation не найден";
pub const FILTER_COLOR_WHEELS_UNAVAILABLE: &str =
    "цветовые колёса недоступны: FFmpeg filter colorbalance не найден";
pub const FILTER_AUDIO_EQ_UNAVAILABLE: &str =
    "аудио EQ недоступен: FFmpeg filter equalizer не найден";
pub const FILTER_PAN_UNAVAILABLE: &str =
    "стереопанорама недоступна: нужны FFmpeg filters aformat и stereotools";
pub const FILTER_COMPRESSOR_UNAVAILABLE: &str =
    "компрессор недоступен: FFmpeg filter acompressor не найден";
pub const FILTER_LIMITER_UNAVAILABLE: &str = "лимитер недоступен: FFmpeg filter alimiter не найден";
pub const FILTER_CHROMA_KEY_UNAVAILABLE: &str =
    "chroma key недоступен: FFmpeg filter chromakey не найден";
pub const FILTER_DESPILL_UNAVAILABLE: &str =
    "подавление chroma spill недоступно: FFmpeg filter despill не найден";
pub const FILTER_LUT3D_UNAVAILABLE: &str = "3D LUT недоступны: FFmpeg filter lut3d не найден";
pub const FILTER_LUT_BLEND_UNAVAILABLE: &str =
    "частичная интенсивность LUT недоступна: FFmpeg filter blend не найден";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_keeps_api_contract_phrases() {
        assert_eq!(INVALID_URL, "Недопустимый URL");
        assert_eq!(QUEUE_CLOSED, "очередь задач закрыта");
        assert_eq!(JOB_ALREADY_FINISHED, "Задача уже завершена");
        assert_eq!(LUT_NOT_FOUND, "LUT не найден");
        assert_eq!(
            BAD_COLOR_PARAMS,
            "некорректные параметры цвета, chroma key или audio DSP"
        );
        assert_eq!(TOO_MANY_JOBS, "Слишком много новых задач; повторите позже");
        assert_eq!(API_ROUTE_NOT_FOUND, "API-маршрут не найден");
        assert_eq!(METHOD_NOT_ALLOWED, "Метод не поддерживается");
    }
}
