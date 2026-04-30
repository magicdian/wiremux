#include <os/log.h>

#include <DriverKit/IOLib.h>
#include <DriverKit/IOUserServer.h>

#include "WiremuxSerialDriver.h"

static uint32_t gBaudRate = 115200;
static bool gDtr = false;
static bool gRts = false;

kern_return_t
IMPL(WiremuxSerialDriver, Start)
{
    kern_return_t ret = Start(provider, SUPERDISPATCH);
    os_log(OS_LOG_DEFAULT, "wiremux serial dext Start: ret=0x%x", ret);
    if (ret == kIOReturnSuccess) {
        ret = RegisterService();
        os_log(OS_LOG_DEFAULT,
               "wiremux serial dext RegisterService: ret=0x%x", ret);
    }
    return ret;
}

kern_return_t
IMPL(WiremuxSerialDriver, Stop)
{
    os_log(OS_LOG_DEFAULT, "wiremux serial dext Stop");
    return Stop(provider, SUPERDISPATCH);
}

kern_return_t
IMPL(WiremuxSerialDriver, ConnectQueues)
{
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext ConnectQueues rxlog=%u txlog=%u",
           in_rxqlogsz, in_txqlogsz);
    return ConnectQueues(ifmd, rxqmd, txqmd, in_rxqmd, in_txqmd,
                         in_rxqoffset, in_txqoffset, in_rxqlogsz,
                         in_txqlogsz, SUPERDISPATCH);
}

kern_return_t
IMPL(WiremuxSerialDriver, DisconnectQueues)
{
    os_log(OS_LOG_DEFAULT, "wiremux serial dext DisconnectQueues");
    return DisconnectQueues(SUPERDISPATCH);
}

void
IMPL(WiremuxSerialDriver, RxFreeSpaceAvailable)
{
    os_log(OS_LOG_DEFAULT, "wiremux serial dext RxFreeSpaceAvailable");
    RxFreeSpaceAvailable(SUPERDISPATCH);
}

void
IMPL(WiremuxSerialDriver, TxDataAvailable)
{
    os_log(OS_LOG_DEFAULT, "wiremux serial dext TxDataAvailable");
    TxDataAvailable(SUPERDISPATCH);
}

kern_return_t
IMPL(WiremuxSerialDriver, HwActivate)
{
    os_log(OS_LOG_DEFAULT, "wiremux serial dext HwActivate");
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwDeactivate)
{
    os_log(OS_LOG_DEFAULT, "wiremux serial dext HwDeactivate");
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwResetFIFO)
{
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwResetFIFO tx=%{BOOL}d rx=%{BOOL}d",
           tx, rx);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwSendBreak)
{
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwSendBreak send=%{BOOL}d", sendBreak);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwProgramUART)
{
    gBaudRate = baudRate;
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwProgramUART baud=%u data=%u halfStop=%u parity=%u",
           baudRate, nDataBits, nHalfStopBits, parity);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwProgramBaudRate)
{
    gBaudRate = baudRate;
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwProgramBaudRate baud=%u", baudRate);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwProgramMCR)
{
    gDtr = dtr;
    gRts = rts;
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwProgramMCR dtr=%{BOOL}d rts=%{BOOL}d",
           dtr, rts);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwGetModemStatus)
{
    if (cts != nullptr) {
        *cts = true;
    }
    if (dsr != nullptr) {
        *dsr = true;
    }
    if (ri != nullptr) {
        *ri = false;
    }
    if (dcd != nullptr) {
        *dcd = true;
    }
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwGetModemStatus baud=%u dtr=%{BOOL}d rts=%{BOOL}d",
           gBaudRate, gDtr, gRts);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwProgramLatencyTimer)
{
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwProgramLatencyTimer latency=%u", latency);
    return kIOReturnSuccess;
}

kern_return_t
IMPL(WiremuxSerialDriver, HwProgramFlowControl)
{
    os_log(OS_LOG_DEFAULT,
           "wiremux serial dext HwProgramFlowControl arg=%u xon=%u xoff=%u",
           arg, xon, xoff);
    return kIOReturnSuccess;
}
