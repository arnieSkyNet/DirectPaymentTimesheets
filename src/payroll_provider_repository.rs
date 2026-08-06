use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone)]
pub struct PayrollProvider {
    pub id: i64,
    pub name: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub telephone: Option<String>,
}

pub struct PayrollProviderRepository {
    connection: Connection,
}

impl PayrollProviderRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn get(&self) -> Result<Option<PayrollProvider>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                name,
                email,
                address,
                telephone
            FROM payroll_provider
            LIMIT 1
            ",
        )?;

        let mut rows = statement.query([])?;

        if let Some(row) = rows.next()? {
            Ok(Some(PayrollProvider {
                id: row.get(0)?,
                name: row.get(1)?,
                email: row.get(2)?,
                address: row.get(3)?,
                telephone: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn save(&self, provider: &PayrollProvider) -> Result<()> {
        self.connection.execute(
            "
            INSERT INTO payroll_provider (
                id,
                name,
                email,
                address,
                telephone
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                email = excluded.email,
                address = excluded.address,
                telephone = excluded.telephone
            ",
            params![
                provider.id,
                provider.name,
                provider.email,
                provider.address,
                provider.telephone,
            ],
        )?;

        Ok(())
    }
}
