use super::*;

#[test]
fn revised_stage2c_effective_rounding_uses_work_date_and_freezes_old_credit() {
    let (_dir,mut app)=setup();
    let (old,weeks)=period(&app,1,"2026-04-01",1);
    app.payroll_worked_item_repository.set_manual_adjustment(
        old.id,&crate::payroll_worked_item_repository::ManualHoursAdjustment {
            week_number:1,adjustment_minutes:60,reason:Some("estimate".into()),
        },"fictional estimate",
    ).unwrap();
    submit(&app,&old,&weeks);
    direct(&app,"2026-04-02T09:00","2026-04-02T10:02");
    let (next,dates)=period(&app,2,"2026-04-29",1);
    assert_eq!(reconciliation::calculate(&app,&next,&dates,None).unwrap().previous_cycle_minutes,15);
    submit(&app,&next,&dates);
    app.context.config.payroll.set_rounding(30,"Up",NaiveDate::from_ymd_opt(2026,5,28).unwrap());
    direct(&app,"2026-05-27T09:00","2026-05-27T10:02");
    direct(&app,"2026-05-28T09:00","2026-05-28T10:02");
    let (later,dates)=period(&app,3,"2026-05-27",1);
    let hours=reconciliation::calculate(&app,&later,&dates,None).unwrap();
    assert_eq!(hours.previous_cycle_minutes,0);
    assert_eq!(hours.week_totals_minutes,[165,0,0,0]);
}

#[test]
fn revised_stage2c_manual_preparation_uses_original_entry_date() {
    let (_dir,app)=setup();
    let (old,_)=period(&app,1,"2026-04-01",1);
    settle(&app,&old);
    direct(&app,"2026-04-02T09:00","2026-04-02T10:00");
    let (next,dates)=period(&app,2,"2026-04-29",1);
    reconciliation::TEST_TODAY.with(|clock| clock.set(Some(dates[1])));
    let preview=manual_preparation::preview(&app,&next,&dates).unwrap();
    manual_preparation::save(&app,&next,&dates,&preview,[60,0,120,0],&[]).unwrap();
    reconciliation::TEST_TODAY.with(|clock| clock.set(Some(dates[3]+chrono::Duration::days(7))));
    let hours=reconciliation::calculate_for_generation(&app,&next,&dates,None).unwrap();
    assert_eq!(hours.snapshot_items.iter().map(|i|i.reason.as_deref()).collect::<Vec<_>>(),
        [Some("Manual confirmed worked hours"),Some("Manual current-period estimate")]);
    submit_manual_candidate(&app,&next,&hours);
    let db=open(&app).unwrap();
    let sid=lifecycle::latest(&db,next.id).unwrap().unwrap();
    assert_eq!(lifecycle::items(&db,sid).unwrap().iter().map(|i|i.reason.as_deref()).collect::<Vec<_>>(),
        [Some("Manual confirmed worked hours"),Some("Manual current-period estimate")]);
    let changes = reconciliation::changes(&app,&next).unwrap();
    assert_eq!(changes.components.iter().map(|c| c.minutes).collect::<Vec<_>>(), [-120],
        "the confirmed past actual is not deducted when CSV evidence is absent");
    reconciliation::TEST_TODAY.with(|clock| clock.set(None));
}

fn submit_manual_candidate(app:&Application, record:&PayrollTimesheet, hours:&crate::pay_rate_allocation::ReconciledPayrollHours) {
    let ids=app.payroll_timesheet_repository.get_weeks(record.id).unwrap().iter().map(|w|w.id).collect::<Vec<_>>().try_into().unwrap();
    let path=app.context.environment.data_dir.join("synthetic-manual.pdf");
    crate::payroll_snapshot_service::publish_candidate(&app.payroll_worked_item_repository,
        crate::payroll_snapshot_service::CandidatePublication{payroll_timesheet_id:record.id,
            items:&hours.snapshot_items,final_pdf_path:&path,generated_at:"synthetic",
            previous_cycle_minutes:hours.previous_cycle_minutes,week_ids:&ids,
            week_totals_minutes:&hours.week_totals_minutes},
        |p| {std::fs::write(p,b"%PDF synthetic manual")?;Ok(())}).unwrap();
    crate::payroll_snapshot_service::send_production_candidate(&app.payroll_worked_item_repository,record.id,
        record.personal_assistant_id,&record.payroll_year,record.cycle_number,&path,"synthetic",||Ok(())).unwrap();
}

fn synthetic_pdf(path:&std::path::Path,text:&[&str]) {
    use lopdf::{dictionary,Document,Object,Stream};
    use lopdf::content::{Content,Operation};
    let mut doc=Document::with_version("1.4");
    let pages=doc.new_object_id();
    let font=doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let resources=doc.add_object(dictionary!{"Font"=>dictionary!{"F1"=>font}});
    let mut ops=vec![Operation::new("BT",vec![]),Operation::new("Tf",vec!["F1".into(),12.into()]),
        Operation::new("Td",vec![72.into(),720.into()])];
    for l in text {
        ops.push(Operation::new("Tj",vec![Object::string_literal(*l)]));
        ops.push(Operation::new("Td",vec![0.into(),(-18).into()]));
    }
    ops.push(Operation::new("ET",vec![]));
    let content=doc.add_object(Stream::new(dictionary!{},Content{operations:ops}.encode().unwrap()));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"Contents"=>content});
    doc.objects.insert(pages,dictionary!{"Type"=>"Pages","Kids"=>vec![page.into()],
        "Count"=>1,"Resources"=>resources,"MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()]}.into());
    let catalog=doc.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root",catalog);
    doc.save(path).unwrap();
}

#[test]
fn revised_stage2c_payslip_import_fallback_is_cycle_scoped_and_nonblocking() {
    for readable in [true,false] {
        let (dir,mut app)=setup();
        app.context.config.folders.payslip_folder=dir.path().join("payslips");
        let (_old,_)=period(&app,1,"2026-04-01",1);
        let schedule=app.payroll_schedule_repository.get_for_year_and_cycle("2026/27",1).unwrap().unwrap();
        let filename=crate::payroll_file_naming::payslip_filename("Example PA",&schedule).unwrap();
        let path=dir.path().join(filename);
        if readable {
            synthetic_pdf(&path,&["Description Hours Rate Amount","Hourly Rate 1 12 12",
                "PubHol X 1.5 1 18 18","PrevPay Rate 1 11 11","Holiday 4 12 48","SSP added 35",
                "Total Payments 124"]);
        } else {
            synthetic_pdf(&path,&["Hourly Rate 12.00","Total Payments 250.00"]);
        }
        let imported=app.import_payroll_documents(&path,Some(&schedule)).unwrap();
        assert_eq!(imported.payslips_imported,1,"{:?}",imported.failures);
        direct(&app,"2026-04-02T09:00","2026-04-02T12:00");
        direct(&app,"2026-04-30T09:00","2026-04-30T10:00");
        let (current,dates)=period(&app,2,"2026-04-29",1);
        let plan=reconciliation::plan(&app,&current).unwrap();
        assert!(plan.notices.iter().any(|n| n.contains(if readable {"3.00 paid working hours"} else {"could not be verified"})),
            "{:?}",plan.notices);
        assert_eq!(reconciliation::calculate(&app,&current,&dates,None).unwrap().week_totals_minutes,[60,0,0,0]);
        submit(&app,&current,&dates);
        let db=open(&app).unwrap();
        assert!(lifecycle::latest(&db,current.id).unwrap().is_some());
    }
}
