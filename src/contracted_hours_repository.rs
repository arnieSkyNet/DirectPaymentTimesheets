use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone)]
pub struct ContractedHoursEntry {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub effective_date: String,
    pub contracted_hours: String,
    pub created_at: String,
}

pub struct ContractedHoursRepository {
    connection: Connection,
}

impl ContractedHoursRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, entry: &ContractedHoursEntry) -> Result<()> {
        let effective_date = normalise_date(&entry.effective_date);

        self.connection.execute(
            "
            INSERT INTO personal_assistant_contracted_hours (
                personal_assistant_id,
                effective_date,
                contracted_hours,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4)
            ",
            params![
                entry.personal_assistant_id,
                effective_date,
                entry.contracted_hours,
                entry.created_at,
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, entry: &ContractedHoursEntry) -> Result<()> {
        let effective_date = normalise_date(&entry.effective_date);

        self.connection.execute(
            "
            UPDATE personal_assistant_contracted_hours
            SET
                effective_date = ?1,
                contracted_hours = ?2
            WHERE id = ?3
            ",
            params![effective_date, entry.contracted_hours, entry.id,],
        )?;

        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.connection.execute(
            "
            DELETE FROM personal_assistant_contracted_hours
            WHERE id = ?1
            ",
            params![id],
        )?;

        Ok(())
    }

    pub fn get_all_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Vec<ContractedHoursEntry>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                personal_assistant_id,
                effective_date,
                contracted_hours,
                created_at
            FROM personal_assistant_contracted_hours
            WHERE personal_assistant_id = ?1
            ORDER BY
                substr(effective_date, 7, 4) DESC,
                substr(effective_date, 4, 2) DESC,
                substr(effective_date, 1, 2) DESC
            ",
        )?;

        let entries = statement.query_map(params![personal_assistant_id], |row| {
            Ok(ContractedHoursEntry {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                effective_date: row.get(2)?,
                contracted_hours: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();

        for entry in entries {
            results.push(entry?);
        }

        Ok(results)
    }
    pub fn get_current_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Option<ContractedHoursEntry>> {
        let mut hours = self.get_all_for_personal_assistant(personal_assistant_id)?;

        if hours.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hours.remove(0)))
        }
    }
}

fn normalise_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('/').collect();

    if parts.len() == 3 {
        let day = parts[0].parse::<u32>().unwrap_or(0);
        let month = parts[1].parse::<u32>().unwrap_or(0);
        let year = parts[2].parse::<u32>().unwrap_or(0);

        if day > 0 && month > 0 && year > 0 {
            return format!("{:02}/{:02}/{:04}", day, month, year);
        }
    }

    date.to_string()
}
